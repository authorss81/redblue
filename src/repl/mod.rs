pub mod banner;
pub mod bytecode_view;
pub mod commands;
pub mod completer;
pub mod editor;
pub mod highlight;
pub mod history;
pub mod introspect;
pub mod measure;
pub mod style;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{self, Write};
use std::rc::Rc;
use std::time::{Duration, Instant};

pub use commands::ReplCommand;
pub use completer::ReplCompleter;
pub use editor::{Line, LineSource, SessionNames};
pub use history::ReplHistory;
pub use introspect::Definition;

use crate::analyzer;
use crate::debugger::{lock, DebugAction, DebugSession, SharedDebugSession, StepMode};
use crate::error::Error;
use crate::interpreter::Vm;
use crate::lexer::{Lexer, TokenKind};
use crate::parser::{open_block_depth, Expr, Parser, Program, Stmt};
use crate::repl::style::{Paint, Style};
use crate::stdlib;
use crate::value::{FunctionBody, Value};

/// How many lines a session keeps before the oldest is dropped. A terminal
/// scrollback holds far more, so a session that outgrows this has already lost
/// anything the user would go looking for.
const HISTORY_LIMIT: usize = 1000;

/// The most lines a block may hold before the session stops waiting for its
/// `end`.
///
/// Nobody types five hundred lines into one block by accident, and a piped block
/// that is never closed — `printf 'repeat 3 times\n' | rb`, or a generator that
/// forgot its `end` — buffered without limit until the process ran out of memory.
/// Past this the session says the block is incomplete and starts over, which is
/// what the user would have to do by hand anyway.
const MAX_BLOCK_LINES: usize = 500;

/// The most bytes a block may hold, for the same reason and for the same inputs:
/// a block of a few very long lines gets nowhere near [`MAX_BLOCK_LINES`] and can
/// still be enormous.
const MAX_BLOCK_BYTES: usize = 64 * 1024;

/// How many lines of an unclosed block are echoed back when the session ends or
/// gives up. The rest are counted and reported, so the report itself cannot be
/// the thing that floods a terminal.
const BLOCK_REPORT_LINES: usize = 20;

/// The prompt a program stopped by the debugger waits at.
///
/// Distinct from `>>> ` and `..  2 ` because it is asked by a running program
/// rather than typed at, and a session has to be able to tell the three apart.
const DEBUG_PROMPT: &str = "(rb-debug) ";

/// The prompt a session reads a fresh line at, until `:prompt` says otherwise.
///
/// A format string rather than a fixed string, because the prompt is the one
/// piece of a session that belongs to the user: they are the one typing at it.
/// What it may contain is [`PROMPT_PLACEHOLDERS`].
const DEFAULT_PROMPT: &str = ">>> ";

/// Every placeholder a `:prompt` format string may name.
///
/// Named and closed, so `:prompt {engine} ` renders rather than printing a
/// literal `{}` at the user, and a typo is refused by name instead of being
/// silently passed through.
const PROMPT_PLACEHOLDERS: [&str; 3] = ["{engine}", "{version}", "{history}"];

/// The most bytes a `:prompt` format may hold.
///
/// A format is kept for as long as the session lasts and rendered once for every
/// line the session reads, so an unbounded one is an unbounded allocation per
/// line — one piped line of a million `x`es would put a megabyte in front of
/// every prompt for the rest of the session. The blocks a session buffers are
/// capped for the same reason and by the same argument
/// ([`MAX_BLOCK_LINES`], [`MAX_BLOCK_BYTES`]); this is the prompt's share of it.
///
/// A kilobyte is far more prompt than anyone types. `>>> ` is four bytes.
const MAX_PROMPT_BYTES: usize = 1024;

/// The line that runs the line before it again.
///
/// Both Ipp and inscript have it, and it is the one input a user reaches for
/// after a long paste: re-running the last line is a keystroke, and re-typing it
/// is a scroll back through the editor's history looking for it.
const REPEAT: &str = "!!";

/// The answers a debugger prompt takes, and what each one asks the VM to do.
///
/// Read in [`debug_stop`]. Every word is also reachable with a sigil, because
/// the same words are the REPL's `:continue`, `:step` and `:next` and a user who
/// has learned one should not have to unlearn it in the other place.
fn debug_answer(input: &str) -> Option<DebugAction> {
    let word = input
        .trim()
        .trim_start_matches([':', '.'])
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    match word.as_str() {
        "c" | "continue" | "r" | "run" | "q" | "quit" | "detach" => Some(DebugAction::Run),
        "s" | "step" | "into" => Some(DebugAction::Step(StepMode::Into)),
        "n" | "next" | "over" => Some(DebugAction::Step(StepMode::Over)),
        _ => None,
    }
}

/// Whether a `:prompt` format string is one a session will accept.
///
/// Refused rather than rendered, because a prompt is printed before every single
/// line: a format string with `{nope}` in it would put that text in front of the
/// user once per line, forever, and the mistake would be invisible in a
/// transcript of anything long. The size is refused for the same reason and by
/// the same argument — see [`MAX_PROMPT_BYTES`] — and the refusal quotes the size
/// rather than the format, so saying "this is too long" cannot itself be the
/// flood.
fn check_prompt_format(format: &str) -> Result<(), String> {
    if format.len() > MAX_PROMPT_BYTES {
        return Err(format!(
            "Error: a prompt format may be at most {MAX_PROMPT_BYTES} bytes, and \
             this one is {}.",
            format.len()
        ));
    }
    let mut rest = format;
    loop {
        let open = rest.find('{');
        let close = rest.find('}');
        // Nothing left to open: whatever is left is display text, and a `}` with
        // no `{` in front of it closes nothing. Refused for the reason the
        // unknown placeholder is: the placeholders are a closed set, so a brace
        // is either one of them or a mistake, and a mistake printed before every
        // line is a mistake forever.
        let Some(open) = open else {
            return if close.is_some() {
                Err(format!(
                    "Error: ':prompt {format}' closes a '}}' it never opens."
                ))
            } else {
                Ok(())
            };
        };
        if close.is_some_and(|close| close < open) {
            return Err(format!(
                "Error: ':prompt {format}' closes a '}}' it never opens."
            ));
        }
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            return Err(format!(
                "Error: ':prompt {format}' opens a '{{' it never closes."
            ));
        };
        let name = &after[..close];
        if !PROMPT_PLACEHOLDERS.contains(&format!("{{{name}}}").as_str()) {
            return Err(format!(
                "Error: ':prompt' does not know the placeholder {{{name}}}. \
                 It knows: {}.",
                PROMPT_PLACEHOLDERS.join(", ")
            ));
        }
        rest = &after[close + 1..];
    }
}

/// Whether `input` at a debugger prompt asked to be shown the frame again.
fn debug_asks_for_frame(input: &str) -> bool {
    let word = input
        .trim()
        .trim_start_matches([':', '.'])
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    matches!(word.as_str(), "f" | "frame" | "where" | "bt" | "backtrace")
}

/// The interactive half of a stop: print where the program is, then read what to
/// do about it.
///
/// This is the one place a running Redblue program reads the terminal, and it is
/// here rather than anywhere in the interpreter so that the VM never learns that
/// a terminal exists. Two reads end the wait rather than spinning on it: end of
/// input, which is what a piped session runs into, and a terminal that has gone
/// away. A debugger that kept asking a closed pipe for answers would hang the
/// very sessions it is meant to be able to watch.
fn debug_stop(report: &[String]) -> DebugAction {
    for line in report {
        println!("{}", line);
    }
    loop {
        print!("{}", DEBUG_PROMPT);
        if io::stdout().flush().is_err() {
            return DebugAction::Run;
        }
        let mut input = String::new();
        match io::stdin().read_line(&mut input) {
            Ok(0) | Err(_) => {
                println!();
                return DebugAction::Run;
            }
            Ok(_) => {}
        }
        if debug_asks_for_frame(&input) {
            for line in report {
                println!("{}", line);
            }
            continue;
        }
        if let Some(action) = debug_answer(&input) {
            return action;
        }
        if input.trim().is_empty() {
            continue;
        }
        println!(
            "Expected c (continue), s (step into), n (step over) or f (frame). \
             A line with none of those is not a debugger answer."
        );
    }
}

/// Reads the line number a `:break` or `:delete` was given, or says why it is
/// not one.
///
/// Nothing is guessed. An argument that does not parse is reported rather than
/// read as line `0`, and `0` is refused rather than clamped up to `1`: a
/// breakpoint on a line the program never has is a debugger that appears to
/// ignore the user, which is worse than one that says the line is wrong.
fn line_number(argument: &str) -> Result<usize, String> {
    let trimmed = argument.trim();
    let line: usize = trimmed
        .parse()
        .map_err(|_| format!("'{trimmed}' is not a line number"))?;
    if line == 0 {
        return Err("lines are counted from 1, so there is no line 0".to_string());
    }
    Ok(line)
}

/// The keywords a name is *declared* by, rather than read through.
///
/// `set x to 1` binds `x`, `constant x`, `to x(..)`, `import M` and `module M`
/// each bind one name, and every one of them is followed by an identifier where a
/// *new* name is written — which is what makes a flat scan of the tokens able to
/// say where a name was declared without knowing what it was bound to. This is
/// what [`Repl::record_definitions`] looks for, so the line it points at is
/// the line the name was written on and not the line it was last used on — the
/// difference between `.which x` naming where `x` came from and naming some
/// earlier line that happened to mention it.
fn declares_a_name(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Set
            | TokenKind::Constant
            | TokenKind::To
            | TokenKind::Module
            | TokenKind::Import
    )
}

/// The program `code` is, without running or analyzing it.
///
/// Shared by the two callers that want the *shape* of what was typed rather than
/// what it did: a typed line runs it, and `.type` reads it to decide whether it
/// is a question at all.
fn parse_only(code: &str) -> Result<Program, crate::error::Error> {
    let tokens = Lexer::tokenize(code)?;
    Parser::new(tokens).parse()
}

/// The name the keyword at `tokens[index]` declares, if it is written as one.
///
/// `None` for anything else, and for `set x.y to …` — there the identifier after
/// the `set` is the *object* being written to, not a name that line declares,
/// and recording it would move `.which x` to a line that only assigned into it.
fn declared_after(tokens: &[crate::lexer::Token], index: usize) -> Option<String> {
    let name = match &tokens.get(index + 1)?.kind {
        TokenKind::Identifier(name) => name.clone(),
        _ => return None,
    };
    if matches!(
        tokens.get(index + 2).map(|token| &token.kind),
        Some(TokenKind::Dot)
    ) {
        return None;
    }
    Some(name)
}

/// Whether the `to` at `tokens[index]` introduces a value rather than declaring a
/// name.
///
/// `set x to 1` holds two words that look like a declaration: `set x`, and a
/// `to` that introduces the value. Only the first declares. The `to` that does
/// declare is the one opening a statement — `to double(n)` — and that one is
/// never two tokens behind a `set` or a `constant`.
fn introduces_a_value(tokens: &[crate::lexer::Token], index: usize) -> bool {
    index >= 2 && matches!(tokens[index - 2].kind, TokenKind::Set | TokenKind::Constant)
}

/// Every name an `import` statement binds, from the tokens after the keyword.
///
/// The whole rest of the line, because one `import` can bind several names and
/// the grammar says so: `import files, network as net` binds `files`, `network`
/// and `net`, and `import json as J` binds `J` as well as leaving `json` naming
/// it. Both kinds of name are bound in the VM, so both are recorded — a scan
/// that read only the identifier after the `import` recorded the first item and
/// called every other one unknown.
fn imported_names(rest: &[crate::lexer::Token]) -> Vec<(String, usize)> {
    let mut names = Vec::new();
    for token in rest {
        match &token.kind {
            // The statement ends at the line's end: whatever is written below it
            // belongs to another statement, and an `import` says nothing about
            // it.
            TokenKind::Newline | TokenKind::Eof => break,
            TokenKind::Identifier(name) => names.push((name.clone(), token.line)),
            _ => {}
        }
    }
    names
}

/// The snippets `:help` prints under "Quick Reference", each one a complete
/// program the session has run.
///
/// Kept as source rather than as a hand-written sentence so the help cannot
/// promise something the interpreter does not do: `if x > 5 then` was printed for
/// years and is not Redblue, and `say "Hello, {{name}}!"` promised an
/// interpolation `say` does not perform.
const QUICK_REFERENCE: &[(&str, &str)] = &[
    ("Variable", "set x to 10"),
    ("Print", r#"say "Hello""#),
    ("Condition", "if x is greater than 5 then say \"big\" end"),
    ("Loop", "for each i from 1 to 3\n    say i\nend"),
];

/// The code `:example` shows: a caption, the program, and the text that program
/// prints.
///
/// Every entry has run in this session before it is printed here — the test at
/// the bottom of this module analyzes each source and asserts it is accepted —
/// so an example cannot promise a feature the interpreter does not have. The old
/// examples promised string interpolation: `say "Hello, {{name}}!"` printed
/// `Hello, {{name}}!`.
const EXAMPLES: &[(&str, &str)] = &[
    ("// Variables", "set name to \"World\"\nsay name"),
    (
        "// Print",
        "set name to \"World\"\nsay \"Hello, \" + name + \"!\"",
    ),
    ("// Math", "set x to (10 + 5) * 2\nsay x"),
    (
        "// Conditionals",
        "set age to 30\nif age is greater than 18 then\n    say \"Adult\"\nelse\n    say \"Minor\"\nend",
    ),
    (
        "// Loops",
        "repeat 3 times\n    say \"Hello!\"\nend",
    ),
    (
        "// Functions",
        "to greet(who)\n    say \"Hello, \" + who + \"!\"\nend\ngreet(\"World\")",
    ),
];

/// Why a session's read loop stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionEnd {
    /// `:quit`. The user asked for it.
    Quit,
    /// Ctrl-D on an empty line, or input that ran out.
    Eof,
    /// The line source could not read. Not a quit: the session did not stop
    /// because it was told to.
    InputError,
}

/// What a session says as it ends.
///
/// Every ending says the same thing, and has since the REPL existed — the
/// differences are why a session ended, which the user just saw happen. Kept as
/// a function of the ending so that is a thing a test can pin rather than a
/// `println!` nothing can observe.
fn farewell(end: SessionEnd) -> &'static str {
    match end {
        SessionEnd::Quit | SessionEnd::Eof | SessionEnd::InputError => "Goodbye!",
    }
}

/// Renders a failure the session is about to report, without the `Error: ` the
/// caller prefixes it with.
///
/// The whole language already knows how to draw a failure: [`Error::render`]
/// writes the label, the message, the `--> file:line:column` location, the
/// offending line of the source and a caret under the column that failed. A
/// session printed the first two through [`std::fmt::Display`] and stopped
/// there, so the same program showed more under `rb run` than at the prompt —
/// and the prompt is where a reader tries a line and looks to see what it did.
///
/// The buffer, not the history, is the source: a block the user typed over
/// several prompts is one program, and the line that failed is line *n* of the
/// block rather than a line of the session.
///
/// A function rather than a `println!` because what a session prints is
/// otherwise nothing a test can read.
fn render_error(style: &Style, code: &str, file: &str, error: &Error) -> String {
    paint_diagnostic(style, &error.render(code, Some(file)))
}

/// Paints the parts of a rendered failure that are about the failure — the
/// label and message, the location, and the caret — and leaves the source line
/// between them exactly as the user wrote it.
///
/// Colour wraps a diagnostic; it never rewrites one. So a line is painted whole
/// or not at all, and with colour off this is the identity on the rendered text:
/// what a session prints about a failure is byte for byte what `rb run` prints
/// for the same program.
fn paint_diagnostic(style: &Style, rendered: &str) -> String {
    rendered
        .split('\n')
        .enumerate()
        .map(|(index, line)| {
            if index == 0 || line.trim_start().starts_with("-->") || is_caret_line(line) {
                style.paint(Paint::Error, line)
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Whether `line` is a drawn failure's caret or underline rather than a line of
/// the program: the gutter of a source line carries its number, and the gutter
/// of a caret carries only the spaces that line up with it.
///
/// The marker past the gutter is required as well as the blank gutter. A gutter
/// is blank on the notice [`Error::render`] leaves when it cuts a range short
/// (`… 3 more line(s) not drawn`) — that is the renderer saying something about
/// the drawing, not part of the drawing, and painting it as a failure would
/// colour the reader's own report that the diagnostic was cut.
fn is_caret_line(line: &str) -> bool {
    match line.split_once(" | ") {
        Some((gutter, marked)) => {
            !gutter.is_empty() && gutter.chars().all(|c| c == ' ') && marked.contains(['^', '~'])
        }
        None => false,
    }
}

/// One line of `Error: ` plus a rendered failure, for a session printing a
/// failure a *command* ran rather than one the run path found.
///
/// The same prefix, the same renderer and the same colour contract as
/// [`Repl::execute_code_from`], so `.type` and `.time` do not answer a
/// misspelling with a worse picture of it than typing the line would.
fn failure_line(style: &Style, code: &str, error: &Error) -> String {
    format!(
        "Error: {}",
        render_error(style, code, introspect::SESSION_FILE, error)
    )
}

pub struct Repl {
    vm: Vm,
    variables: HashMap<String, Value>,
    history: ReplHistory,
    completer: ReplCompleter,
    running: bool,
    multiline_buffer: Vec<String>,
    in_multiline: bool,
    /// The debugging session: its breakpoints, its pending step, and the frame
    /// the last program stopped in. Shared with the hook the VM calls, so a
    /// `:frame` typed after the program has run reads the frame that stop left.
    debug: SharedDebugSession,
    /// How output is coloured. Detected once at startup, and `forced` in
    /// tests so a colour assertion can run without a TTY.
    style: Style,
    /// The format the primary prompt is rendered from, and what `:prompt` sets.
    ///
    /// Uncoloured text: the reader paints it, because a prompt carrying escape
    /// sequences into a line editor is the editor's problem to measure, not
    /// something to hand it raw.
    prompt_format: String,
    /// Where each name this session bound was written, for `.type` and `.which`.
    ///
    /// The VM knows *that* a name is bound and the session knows *where* it was
    /// typed, and neither knew the other's half: `:inspect x` printed the value
    /// and its kind and nothing about the line three prompts ago that made it.
    definitions: HashMap<String, Definition>,
}

impl Repl {
    pub fn new() -> Self {
        Self {
            vm: Vm::new(),
            variables: HashMap::new(),
            history: ReplHistory::new(HISTORY_LIMIT),
            completer: ReplCompleter::new(),
            running: true,
            multiline_buffer: Vec::new(),
            in_multiline: false,
            debug: DebugSession::shared(),
            style: Style::detect(),
            prompt_format: DEFAULT_PROMPT.to_string(),
            definitions: HashMap::new(),
        }
    }

    /// Completes `word` against the session: the language's own keywords and
    /// builtins, the REPL's commands, and every name this session has bound.
    pub fn complete(&self, word: &str) -> Vec<String> {
        self.completer.complete_with(word, &self.session_names())
    }

    /// Every name a completion could offer from the live session: the REPL's own
    /// variables, plus the VM's own bindings, which is where a `to` function and
    /// an `import`ed module's own `set` and `constant` declarations live.
    ///
    /// The `module_member` names a module's functions are published under are not
    /// here — see [`Vm::user_names`]. `MathUtils.circle_area` is what a program
    /// writes, and `MathUtils_circle_area` is only how the VM reaches it.
    fn session_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.variables.keys().cloned().collect();
        names.extend(self.vm.user_names());
        names.sort();
        names.dedup();
        names
    }

    /// Whether `code` is a program the user has not finished typing, and so wants
    /// the session to read another line before running anything.
    ///
    /// The question is asked of the tokens rather than of the last word of the
    /// line. Matching the last word got both directions wrong: `repeat 3 times`
    /// ends in `times` and `for each i from 1 to 10` ends in `10`, so real blocks
    /// were run half-written, while a stray `end` matched and captured every line
    /// after it.
    ///
    /// A line the lexer cannot read is treated as finished. It is a line the user
    /// is going to retype, and waiting for an `end` that will never balance it
    /// would hang the session on it.
    fn needs_continuation(code: &str) -> bool {
        match Lexer::tokenize(code) {
            Ok(tokens) => open_block_depth(&tokens) > 0,
            Err(_) => false,
        }
    }

    pub fn run(&mut self) {
        // The wordmark, then the greeting. Painted once, never redrawn.
        let mut out = std::io::stdout();
        if let Err(e) = banner::print_banner(&mut out, &self.style) {
            eprintln!("Warning: could not print banner: {}", e);
        }
        if let Err(e) = banner::print_greeting(&mut out, &self.style) {
            eprintln!("Warning: could not print greeting: {}", e);
        }

        let names: SessionNames = Rc::new(RefCell::new(Vec::new()));
        let mut source = editor::line_source(Rc::clone(&names), self.style);
        self.restore_history(&*source);
        let end = self.session_loop(&mut *source, &names);
        if let Err(error) = source.flush_history() {
            eprintln!(
                "{}",
                self.style.paint(
                    Paint::Error,
                    &format!("Warning: could not save history: {}", error)
                )
            );
        }

        println!("{}", self.style.paint(Paint::Heading, farewell(end)));
    }

    /// Reads back what a previous session left behind, so `:history` answers
    /// about the lines this one can actually walk back to.
    ///
    /// The lines come from the line source rather than from re-reading its file:
    /// the editor's history file is *its* format, and reading it as one line per
    /// entry would put its version header in front of the user as if they had
    /// typed it.
    fn restore_history(&mut self, source: &dyn LineSource) {
        for line in source.history() {
            self.history.push(line);
        }
    }

    /// The read-evaluate-print loop, over whatever is reading the lines.
    ///
    /// Nothing here knows about terminals: the interrupt that abandons a line,
    /// the end of input that ends the session and the failed read that ends it
    /// early all arrive as a [`Line`], which is what lets a test drive all three
    /// without one.
    pub fn session_loop<S: LineSource + ?Sized>(
        &mut self,
        source: &mut S,
        names: &SessionNames,
    ) -> SessionEnd {
        loop {
            if !self.running {
                return SessionEnd::Quit;
            }

            let prompt = self.prompt_for_next_line();

            // The editor completes against the names the session has bound, and
            // it cannot be lent them while they are being borrowed to read a
            // line, so it is given a snapshot refreshed here.
            *names.borrow_mut() = self.session_names();

            let line = match source.read_line(&prompt) {
                Ok(Line::Text(text)) => text,
                // Ctrl-C throws away the line being written, not the session.
                Ok(Line::Interrupted) => continue,
                Ok(Line::Eof) => {
                    self.report_unclosed_block();
                    return SessionEnd::Eof;
                }
                Err(_) => return SessionEnd::InputError,
            };

            // The newline a reader stopped at is not part of the line, and
            // everything below treats the line as if it were not there: what is
            // recorded, what is dispatched as a command, and what is run as
            // Redblue. Left on, it is invisible for source — `trim` eats it — and
            // fatal for a `:prompt` whose argument *is* display text, because the
            // format then ends in a newline and every prompt after it prints a
            // blank line of its own. Stripped here rather than trusted away, so
            // this holds for any source; only the trailing `\r\n` goes, because a
            // trailing space is part of a prompt and must survive.
            let line = line.trim_end_matches(['\r', '\n']);

            let input = line.trim();
            if input.is_empty() {
                continue;
            }

            // Commands are dispatched from the line as it was typed rather than
            // from its trimmed form, because `:prompt rb> ` ends in a space and
            // that space is the command's argument. Every other command trims
            // what it is given, so nothing else can tell the difference.
            let typed = line.trim_start();

            // Recorded the way it is dispatched, because that is what `!!` hands
            // back: the history entry for `:prompt rb> ` is run again as a
            // command, and recorded trimmed it would come back as `:prompt rb>`
            // and silently drop the space out of the prompt. Only the commands
            // whose argument *is* display text are recorded with their trailing
            // whitespace; everything else is recorded trimmed, so a stray space
            // at the end of a line does not reach the history file either.
            let recorded = if ReplCommand::keeps_argument_whitespace(typed) {
                typed
            } else {
                input
            };
            self.history.push(recorded.to_string());

            // `!!` is a line like any other as far as the history is concerned —
            // it was typed, and `:history` should show it — but what runs is the
            // line before it, dispatched the same way, so `!!` over a command
            // repeats the command rather than trying to evaluate `:vars` as
            // Redblue.
            let expanded = if input == REPEAT {
                match self.repeat() {
                    Ok(previous) => previous,
                    Err(message) => {
                        println!("{}", self.style.paint(Paint::Error, &message));
                        continue;
                    }
                }
            } else {
                input.to_string()
            };

            let dispatched = if input == REPEAT { &expanded } else { typed };
            if self.handle_command(dispatched) {
                continue;
            }

            self.execute_line(&expanded);
        }
    }

    /// The prompt the next line is read at, as plain text the reader paints.
    ///
    /// Two shapes, and the second is numbered: inside a block, `..  2` says this
    /// is line 2 of the block and `..  3` says line 3, where a flat `..  ` said
    /// neither. A block the user pasted from a file, or a traceback quoting line
    /// 17 of one, is only readable if the session's own lines are numbered to
    /// match. The number is the line about to be typed, so it never skips one and
    /// never repeats one.
    ///
    /// Both shapes end in a space, because what is typed lands right after the
    /// prompt: the primary one does, and a continuation that ran into the digits
    /// without a gap put the first character a user types against them.
    fn prompt_for_next_line(&self) -> String {
        if self.in_multiline {
            return format!("..  {} ", self.multiline_buffer.len() + 1);
        }
        self.render_prompt_format()
    }

    /// The user's prompt format with its placeholders filled in.
    ///
    /// Filled by substitution rather than by a formatting library so the whole
    /// surface is the closed set [`PROMPT_PLACEHOLDERS`] — a prompt cannot depend
    /// on anything a format string can reach that this cannot.
    ///
    /// Three passes over a format [`check_prompt_format`] has already bounded to
    /// [`MAX_PROMPT_BYTES`], once per line read. The bound is why that is cheap:
    /// without it, a single `:prompt` line would make every prompt after it
    /// allocate whatever the user had typed, forever.
    fn render_prompt_format(&self) -> String {
        self.prompt_format
            .replace("{engine}", banner::ENGINE)
            .replace("{version}", banner::VERSION)
            .replace("{history}", &self.history.len().to_string())
    }

    /// Sets the prompt from a `:prompt` format string, or restores
    /// [`DEFAULT_PROMPT`] when given no argument.
    ///
    /// The format is checked before it is stored, so a format string that would
    /// print `{nope}` at the user — or one big enough to be printed at them a
    /// thousand times — never becomes the prompt. The session keeps the one it
    /// had, and says which placeholder it did not know or how long the format was.
    fn set_prompt_format(&mut self, argument: Option<&str>) -> Result<(), String> {
        match argument {
            None => {
                self.prompt_format = DEFAULT_PROMPT.to_string();
                Ok(())
            }
            Some(format) => {
                check_prompt_format(format)?;
                self.prompt_format = format.to_string();
                Ok(())
            }
        }
    }

    /// The line `!!` repeats: the last line typed that was not itself `!!`.
    ///
    /// Skipping the repeats is what keeps `!!` usable more than once: a second
    /// `!!` has to reach the same line the first one did, not the `!!` that
    /// reached it.
    fn repeat_target(&self) -> Option<&str> {
        (0..self.history.len())
            .rev()
            .filter_map(|index| self.history.get(index).map(String::as_str))
            .find(|line| !line.trim().is_empty() && line.trim() != REPEAT)
    }

    /// The line `!!` should run, or the message saying there is none.
    ///
    /// An `Err` rather than a default: a session whose first line is `!!` has
    /// nothing to repeat, and reporting that is the only honest answer. An index
    /// into an empty history would panic, and repeating nothing would look like a
    /// command that ran and printed nothing.
    fn repeat(&self) -> Result<String, String> {
        self.repeat_target().map(str::to_string).ok_or_else(|| {
            format!(
                "Error: '{REPEAT}' has nothing to repeat: it is the first line of \
                 this session's history."
            )
        })
    }

    /// Runs one command line, returning whether `input` was a command rather
    /// than Redblue source.
    ///
    /// The command table lives in [`ReplCommand::parse`]; this only dispatches
    /// what it produced. Two copies of the table meant an alias added to one was
    /// invisible to the other.
    fn handle_command(&mut self, input: &str) -> bool {
        match ReplCommand::parse(input) {
            ReplCommand::Quit => {
                self.running = false;
                true
            }
            ReplCommand::Help => {
                self.print_help();
                true
            }
            ReplCommand::Clear => {
                print!("\x1B[2J\x1B[H");
                let _ = io::stdout().flush();
                true
            }
            ReplCommand::History => {
                for i in 0..self.history.len() {
                    if let Some(entry) = self.history.get(i) {
                        println!("  {:4}  {}", i + 1, entry);
                    }
                }
                true
            }
            ReplCommand::Vars => {
                self.print_variables();
                true
            }
            ReplCommand::Functions => {
                self.print_functions();
                true
            }
            ReplCommand::Load(path) => {
                self.load_file(&path);
                true
            }
            ReplCommand::Save(path) => {
                self.save_session(&path);
                true
            }
            ReplCommand::Restore(path) => {
                self.restore_session(&path);
                true
            }
            ReplCommand::Reset => {
                self.reset_session();
                true
            }
            ReplCommand::Run(path) => {
                self.run_file(&path);
                true
            }
            ReplCommand::Debug => {
                self.debug_mode();
                true
            }
            ReplCommand::Break(line) => {
                self.toggle_breakpoint(&line);
                true
            }
            ReplCommand::Breaks => {
                self.print_breakpoints();
                true
            }
            ReplCommand::Delete(line) => {
                self.delete_breakpoint(&line);
                true
            }
            ReplCommand::Step => {
                self.arm_step(StepMode::Into);
                true
            }
            ReplCommand::Next => {
                self.arm_step(StepMode::Over);
                true
            }
            ReplCommand::Continue => {
                self.resume();
                true
            }
            ReplCommand::Frame => {
                self.print_frame();
                true
            }
            ReplCommand::Inspect(name) => {
                self.inspect_value(&name);
                true
            }
            ReplCommand::Type(expression) => {
                self.print_type(&expression);
                true
            }
            ReplCommand::Doc(module) => {
                self.print_module_doc(&module);
                true
            }
            ReplCommand::Which(name) => {
                self.print_which(&name);
                true
            }
            ReplCommand::Sighelp(name) => {
                self.print_sighelp(&name);
                true
            }
            ReplCommand::Time(expression) => {
                self.print_time(&expression);
                true
            }
            ReplCommand::Bench(expression) => {
                self.print_bench(&expression);
                true
            }
            ReplCommand::Ast(expr) => {
                self.print_ast(&expr);
                true
            }
            ReplCommand::Tokens(expr) => {
                self.print_tokens(&expr);
                true
            }
            ReplCommand::Bytecode(expr) => {
                self.print_bytecode(&expr);
                true
            }
            ReplCommand::Asm(expr) => {
                self.print_asm(&expr);
                true
            }
            ReplCommand::Example => {
                self.show_examples();
                true
            }
            ReplCommand::Prompt(argument) => {
                // The whole argument, whitespace inside it included: a prompt is
                // read, not parsed, so `:prompt rb> ` must keep the space the
                // prompt ends with. Hence the command carries an `Option` and the
                // table is not the one that decides it is missing.
                let argument = argument.as_deref();
                if let Err(message) = self.set_prompt_format(argument) {
                    println!("{}", self.style.paint(Paint::Error, &message));
                }
                true
            }
            ReplCommand::NotACommand => false,
            ReplCommand::MissingArgument { command, usage } => {
                println!("Usage: :{} {}", command, usage);
                true
            }
            ReplCommand::Unknown(word) => {
                println!(
                    "Unknown command '{}'. Type :help for available commands.",
                    word
                );
                true
            }
        }
    }

    /// Buffers `input` and runs the buffer once it holds a whole program.
    ///
    /// A block the user opened and has not closed yet leaves the session in
    /// continuation mode; the buffer is run the moment it balances, and a line
    /// that leaves a block open is the only thing that opens continuation mode at
    /// all. That is what makes a block work whichever of its forms the user typed
    /// it in, and what stops a stray `end` from swallowing the rest of the
    /// session.
    fn execute_line(&mut self, input: &str) {
        self.multiline_buffer.push(input.to_string());
        let buffered = self.multiline_buffer.join("\n");
        if Self::exceeds_block_limits(&self.multiline_buffer, buffered.len()) {
            self.report_buffered_block_too_large();
            return;
        }
        if Self::needs_continuation(&buffered) {
            self.in_multiline = true;
            return;
        }

        let code = std::mem::take(&mut self.multiline_buffer).join("\n");
        self.in_multiline = false;
        self.execute_code(&code);
    }

    /// Forgets everything the session has done: its bindings, its last result,
    /// and any block it is halfway through typing.
    ///
    /// The block is the half that is easy to leave out. `:reset` used to replace
    /// the VM and clear `variables` and nothing else, so `repeat 3 times` then
    /// `:reset` left the session in continuation mode with that line still
    /// buffered: the next line was appended to a block nobody was typing any
    /// more, so `say "hi"` waited for an `end` that was never coming and never
    /// ran. A session that has been reset is a session at a prompt, so the buffer
    /// goes with the bindings.
    ///
    /// The recorded definitions go with them, and that half was just as visible.
    /// They are a map from a name to the line that bound it, and a name a reset
    /// session no longer holds was still reported by `.which` as a session
    /// binding at `<session>:3` — a span pointing at a line of a session that had
    /// been thrown away, quoted at that. A reset forgets what it was; a `.which`
    /// that disagrees with it is worse than the stale binding it used to report,
    /// because it is a wrong answer rather than an absence.
    ///
    /// The history is left alone: it is the record of what was typed, and a reset
    /// is not a new session in the sense that the earlier lines were never typed.
    fn reset_session(&mut self) {
        self.vm = Vm::new();
        self.variables.clear();
        self.definitions.clear();
        self.multiline_buffer.clear();
        self.in_multiline = false;
        println!("REPL state reset.");
    }

    /// Whether a buffer of `bytes` over `lines` is too much for a block the user
    /// is still typing.
    ///
    /// Both limits, because either alone leaves a hole: a block of a thousand
    /// blank lines is small in bytes, and one line of a megabyte is one line.
    fn exceeds_block_limits(lines: &[String], bytes: usize) -> bool {
        lines.len() > MAX_BLOCK_LINES || bytes > MAX_BLOCK_BYTES
    }

    /// Gives up on a block that has outgrown what a session will hold, saying so
    /// rather than buffering on.
    ///
    /// The buffer is only unbounded while `needs_continuation` stays true, which
    /// is forever for a piped block whose `end` was never written — or never
    /// typed, because the user walked away. Reporting is better than dying: the
    /// user finds out which block was dropped, and the session carries on reading
    /// the next line instead of holding the rest of the input in memory.
    fn report_buffered_block_too_large(&mut self) {
        let lines = self.multiline_buffer.len();
        let bytes: usize = self
            .multiline_buffer
            .iter()
            .map(|line| line.len() + 1)
            .sum();
        self.multiline_buffer.clear();
        self.in_multiline = false;
        println!(
            "Error: incomplete block: dropping a block of {} line(s) and {} byte(s) \
             that never balanced; a session holds at most {} lines or {} bytes.",
            lines, bytes, MAX_BLOCK_LINES, MAX_BLOCK_BYTES
        );
    }

    /// Reports a block the user opened and never closed, instead of dropping the
    /// lines of it on the floor at EOF.
    ///
    /// Discarding the buffer made an unfinished block and a finished one look the
    /// same from the outside: the session printed `Goodbye!` either way, so a
    /// block that never ran was indistinguishable from one that did.
    ///
    /// At most [`BLOCK_REPORT_LINES`] lines are echoed. The buffer is bounded now,
    /// so this cannot flood, but the count of what was left out is printed rather
    /// than the rest of it — a report is not the place to be the second problem.
    fn report_unclosed_block(&mut self) {
        if self.multiline_buffer.is_empty() {
            return;
        }

        let code = std::mem::take(&mut self.multiline_buffer).join("\n");
        self.in_multiline = false;
        let lines: Vec<&str> = code.lines().collect();
        println!(
            "Error: incomplete block: {} line(s) were never closed with 'end'.",
            lines.len()
        );
        for line in lines.iter().take(BLOCK_REPORT_LINES) {
            println!("  {}", line);
        }
        if lines.len() > BLOCK_REPORT_LINES {
            println!(
                "  ... and {} more line(s)",
                lines.len() - BLOCK_REPORT_LINES
            );
        }
    }

    fn execute_code(&mut self, code: &str) {
        let offset = self.current_line_number().saturating_sub(1);
        self.execute_code_from(introspect::SESSION_FILE, offset, code);
    }

    /// The number `:history` prints beside the line being run, which is the line
    /// a name it binds was written on.
    ///
    /// The history rather than a count of lines executed: `:history` is what the
    /// user is looking at when they ask where something came from, so a span
    /// that did not match it would be a span they could not check.
    fn current_line_number(&self) -> usize {
        self.history.len()
    }

    /// Runs `code` as the program at `file`, whose first line is `line_offset`
    /// plus one.
    ///
    /// The offset is what makes one definition format serve two callers: a line
    /// the user typed has no line of its own but has a place in the history, and
    /// a file's line *is* its line.
    fn execute_code_from(&mut self, file: &str, line_offset: usize, code: &str) {
        let result = self.run_code(code);

        match result {
            Ok(value) => {
                self.record_definitions(file, line_offset, code);
                if !matches!(value, Value::Nothing) {
                    let line = format!("= {}", value);
                    println!("{}", self.style.paint(Paint::Result, &line));
                    self.variables.insert("_".to_string(), value);
                }
            }
            Err(e) => {
                println!("Error: {}", render_error(&self.style, code, file, &e));
            }
        }
    }

    /// Notes where `code` bound each name, for `.type` and `.which`.
    ///
    /// Read out of the source rather than out of the VM's bindings afterwards,
    /// because the difference is the whole point: a `set` of a name that was
    /// already bound changes nothing a before-and-after comparison can see, so
    /// `:which x` reported the line the *first* `set` was on and pointed at a
    /// value that had not been there since. The declaring keyword is what is
    /// looked for — `set x to`, `constant x`, `to x(..)`, `import M`,
    /// `module M` — so the line recorded is the line the name was written on and
    /// not the line it was last read.
    ///
    /// Three things about that list are worth writing down, because each of them
    /// was a name the command said nothing about:
    ///
    /// - **An `import` declares more than one name.** `import files, network as
    ///   net` binds `files`, `network` *and* `net` — the grammar says so, and
    ///   [`crate::parser`] parses a list of items with an optional alias each.
    ///   Reading only the identifier after the `import` recorded the first item
    ///   and left the rest unknown.
    /// - **A `to` in `set x to …` is not a declaration.** It introduces the value.
    ///   `set x to y` overwrote `y`'s line with a line on which `y` was only
    ///   read, which is the exact failure reading the source was meant to avoid.
    /// - **A declared module binds no value of its own.** `module Foo … export
    ///   all end` publishes `Foo_greet`; the name `Foo` is a namespace rather
    ///   than a binding. So "did the run bind it?" is the wrong question for it
    ///   and [`Vm::declares_module`] is the right one — otherwise `.which Foo`
    ///   answered "unknown" for a declaration the session had just run.
    ///
    /// A name the code declares but the run did not bind is not recorded: a
    /// `set` inside a branch that did not run declared nothing, and pointing at
    /// it would be pointing at a line that made no binding.
    fn record_definitions(&mut self, file: &str, line_offset: usize, code: &str) {
        let tokens = match Lexer::tokenize(code) {
            Ok(tokens) => tokens,
            Err(_) => return,
        };
        let lines: Vec<&str> = code.lines().collect();

        // (name, the line it was written on, whether it is a module declaration)
        let mut declared: Vec<(String, usize, bool)> = Vec::new();
        for (index, token) in tokens.iter().enumerate() {
            if !declares_a_name(token.kind.clone()) {
                continue;
            }
            match token.kind {
                TokenKind::Import => declared.extend(
                    imported_names(&tokens[index + 1..])
                        .into_iter()
                        .map(|(name, line)| (name, line, false)),
                ),
                TokenKind::To => {
                    if introduces_a_value(&tokens, index) {
                        continue;
                    }
                    if let Some(name) = declared_after(&tokens, index) {
                        declared.push((name, token.line, false));
                    }
                }
                TokenKind::Module => {
                    if let Some(name) = declared_after(&tokens, index) {
                        declared.push((name, token.line, true));
                    }
                }
                _ => {
                    if let Some(name) = declared_after(&tokens, index) {
                        declared.push((name, token.line, false));
                    }
                }
            }
        }

        for (name, line, module) in declared {
            // A module declares itself; everything else has to have been bound
            // for the line to be a line that bound something.
            let bound = self.lookup(&name).is_some() || (module && self.vm.declares_module(&name));
            if !bound {
                continue;
            }
            let source = lines
                .get(line.saturating_sub(1))
                .copied()
                .unwrap_or_default()
                .trim()
                .to_string();
            self.definitions.insert(
                name,
                Definition {
                    file: file.to_string(),
                    line: line_offset + line,
                    source,
                },
            );
        }
    }

    /// Runs one line as a program of its own, analyzed against the names the
    /// session has already bound.
    ///
    /// The analyzer is seeded from the session rather than started empty, which
    /// is the whole difference between a REPL and a program runner: the VM is
    /// reused across lines, so a `set`, a `to` or an `import` from an earlier line
    /// is still bound here, and without the seed the next line could not read it.
    /// `set x to 1` then `say x` was `AnalyzerError: Unknown variable 'x'` — the
    /// value was right there in the VM and the analyzer had never heard of the
    /// name.
    fn run_code(&mut self, code: &str) -> Result<Value, crate::error::Error> {
        let program = parse_only(code)?;
        let bound = self.session_names();
        analyzer::analyze_with_bound_names(&program, &bound)?;
        self.run_program(&program, code)
    }

    /// Installs the debugger on the session's VM for the program about to run,
    /// or takes it away again.
    ///
    /// The hook is installed only when the session has something to stop for — a
    /// breakpoint, or a step armed at the prompt — so a session that never asked
    /// to debug pays a single `is_none` branch per statement and nothing else.
    fn arm_debugger(&mut self, source: &str) {
        if !lock(&self.debug).active() {
            self.vm.set_debug_hook(None);
            return;
        }
        lock(&self.debug).set_source(source);
        let session = std::sync::Arc::clone(&self.debug);
        self.vm.set_debug_hook(Some(Box::new(move |event| {
            let mut session = lock(&session);
            if !session.poll(&event) {
                return DebugAction::Run;
            }
            let report = session.stop_report();
            let action = debug_stop(&report);
            session.record_action(action);
            action
        })));
    }

    /// Runs `program` in this session's VM and prints what it printed.
    ///
    /// [`Vm::run`] prints the VM's whole output buffer at the end of a program and
    /// leaves the buffer alone, and that buffer belongs to the VM rather than to
    /// one line. Draining it first is what stops `say "A"` on the second line from
    /// coming back on every line after it, which made a session's output grow with
    /// the square of its length.
    ///
    /// Printing what is left on the error path is the other half: a program that
    /// fails half way through never reaches [`Vm::run`]'s print loop, so the `say`
    /// it had already run would otherwise be reported several lines later, by
    /// whichever program ran next.
    /// `source` is the text `program` was parsed from, which is what a stop shows
    /// the user so the line they are looking at is the line they wrote. It is
    /// the whole block the session buffered, so its line numbers are the block's
    /// own rather than the prompt's.
    fn run_program(
        &mut self,
        program: &Program,
        source: &str,
    ) -> Result<Value, crate::error::Error> {
        self.arm_debugger(source);
        self.vm.take_output();
        let result = self.vm.run(program);
        if result.is_err() {
            for line in self.vm.take_output() {
                println!("{}", line);
            }
        }
        result
    }

    fn print_help(&self) {
        println!(
            "{}",
            self.style.paint(Paint::Heading, "Redblue REPL Commands:")
        );
        println!();
        // Every line of this list comes from the one table the parser accepts
        // commands in, aliases included. It was written out separately, so `:q`,
        // `:h`, `:hist`, `:v`, `:f`, `:cls`, `:l`, `:s` and `:i` all worked and
        // none of them was documented.
        for line in ReplCommand::help_lines() {
            println!("{}", line);
        }
        println!();
        // The second sigil is parsed by [`ReplCommand::parse`] and completed by
        // the completer, so a table that lists only `:` leaves half of what the
        // REPL answers to unexplained. One sentence says it once rather than
        // printing every command twice.
        let alt = commands::ALT_SIGIL;
        println!(
            "  Every command also answers to '{alt}' in place of ':' \
             — {alt}quit is :quit."
        );
        println!();
        println!("Quick Reference:");
        for (caption, source) in QUICK_REFERENCE {
            println!("  // {}", caption);
            for line in source.lines() {
                println!("  {}", line);
            }
        }
        println!();
    }

    /// Every name the session has bound, with the value it holds, in name order.
    ///
    /// The REPL's own `variables` map is nearly empty — it holds `_`, the last
    /// result — because a `set`, a `to` and an `import` all bind in the VM, which
    /// is why `:vars` used to be able to print one line for a session that had
    /// bound a dozen names. The VM's bindings are read as well.
    ///
    /// Sorted because both maps are `HashMap`s and their iteration order is not a
    /// result: printing it directly gave one session a different listing from the
    /// next for the same bindings. Built here rather than printed inline so the
    /// order is something a test can read back.
    fn session_bindings(&self) -> Vec<(String, Value)> {
        let mut bindings: HashMap<String, Value> = self.variables.clone();
        for name in self.vm.user_names() {
            if let Some(value) = self.vm.user_value(&name) {
                // The REPL's own entry for a name wins: `_` is the last result,
                // and the VM holds a `Nothing` for it rather than the value.
                bindings.entry(name).or_insert_with(|| value.clone());
            }
        }

        let mut bindings: Vec<(String, Value)> = bindings.into_iter().collect();
        bindings.sort_by(|left, right| left.0.cmp(&right.0));
        bindings
    }

    /// The `:vars` listing, or `None` when the session holds nothing at all.
    fn variable_lines(&self) -> Option<Vec<String>> {
        let bindings = self.session_bindings();
        if bindings.is_empty() {
            return None;
        }

        Some(
            bindings
                .iter()
                .map(|(name, value)| format!("  {} = {}", name, value))
                .collect(),
        )
    }

    fn print_variables(&self) {
        let Some(lines) = self.variable_lines() else {
            println!("No variables defined.");
            return;
        };

        println!("Variables:");
        for line in lines {
            println!("{}", line);
        }
    }

    /// The `:funcs` listing: every function the session declared, in name order,
    /// or `None` when it has declared none.
    ///
    /// It used to print "User-defined functions are stored in the VM" and nothing
    /// else, which named no function and told the user nothing they could not read
    /// off the same line — and the test that ran `:funcs` asserted that sentence,
    /// so the missing listing could not fail anything.
    fn function_lines(&self) -> Option<Vec<String>> {
        let functions = self.vm.user_functions();
        if functions.is_empty() {
            return None;
        }

        Some(functions.iter().map(|name| format!("  {}", name)).collect())
    }

    fn print_functions(&self) {
        let Some(lines) = self.function_lines() else {
            println!("No functions defined.");
            return;
        };

        println!("Functions:");
        for line in lines {
            println!("{}", line);
        }
    }

    /// Reads the file at `path` and runs its source in this session's VM.
    ///
    /// `:load` runs, which is what `:help` promises: "Load and run a file". It
    /// used to read the file and print its size without running anything, so a
    /// session that loaded a file and carried on held none of the bindings the
    /// file declared.
    ///
    /// The source goes through [`Repl::execute_code`] rather than
    /// [`Repl::run_file`], so the bindings it makes are the session's own and
    /// stay available to the lines that follow — the difference from `:run`,
    /// which runs the file in a VM of its own and throws them away.
    fn load_file(&mut self, path: &str) {
        let source = match std::fs::read_to_string(path) {
            Ok(content) => content,
            Err(e) => {
                println!("Could not load file: {}", e);
                return;
            }
        };

        println!("Loaded {} bytes from '{}'.", source.len(), path);
        // The path, not `<session>`: a name this file binds is reported by
        // `.which` as `path:line`, which is a place the user can open.
        self.execute_code_from(path, 0, &source);
    }

    /// Writes the session's history to `path`, one entry per line, in the
    /// format [`ReplHistory::load_from_file`] reads back — and read back by
    /// [`Repl::restore_session`], which is what made the round trip one.
    fn save_session(&self, path: &str) {
        match self.history.save_to_file(path) {
            Ok(()) => println!(
                "Session saved to '{}' ({} line(s)).",
                path,
                self.history.len()
            ),
            Err(e) => println!("Could not save session: {}", e),
        }
    }

    /// Reads a history file written by [`Repl::save_session`] back into this
    /// session, and re-runs what it holds.
    ///
    /// `:save` had no other half: nothing in the REPL ever called
    /// [`ReplHistory::load_from_file`], so a saved session could not be loaded
    /// and the round trip was asserted only between two `ReplHistory`s in a unit
    /// test, never through the commands that write and read the file.
    ///
    /// The file replaces the history rather than being appended to it: `:save`
    /// writes the whole session, so restoring it into a session that has since
    /// typed more lines would give back two sessions at once.
    ///
    /// The lines are *run*, not only listed. A restore that only filled the
    /// history printed "Restored 2 line(s)" and left the session unable to read
    /// what those lines had bound: `set x to 1`, `:save`, a fresh session,
    /// `:restore`, `say x` was still `AnalyzerError: Unknown variable 'x'`. The
    /// lines were back and the state they had made was not, which is the half
    /// `:restore` was named for.
    fn restore_session(&mut self, path: &str) {
        let mut restored = ReplHistory::new(HISTORY_LIMIT);
        if let Err(e) = restored.load_from_file(path) {
            println!("Could not restore session: {}", e);
            return;
        }

        let lines: Vec<String> = (0..restored.len())
            .filter_map(|index| restored.get(index).cloned())
            .collect();
        let restored_lines = lines.len();
        self.history = restored;

        let (replayed, skipped) = self.replay(&lines);
        println!("Restored {} line(s) from '{}'", restored_lines, path);
        if restored_lines > 0 {
            println!(
                "Re-ran {} of them; {} REPL command(s) were listed and not re-run.",
                replayed, skipped
            );
        }
    }

    /// Runs `lines` in this session in the order they were typed, as the read
    /// loop would have run them, and reports how many were Redblue source and how
    /// many were REPL commands.
    ///
    /// The lines are run one at a time rather than as one program, so a line that
    /// fails is reported and the lines after it are still restored — and so a
    /// block that was typed over several lines comes back whole, through
    /// [`Repl::execute_line`]'s buffering.
    ///
    /// A line that was a command is *not* run. `:save`, `:vars` and `:history`
    /// are about the session rather than about its bindings, and the other four
    /// do what they say at the user: a restore that cleared the screen, wrote a
    /// file or quit the session would be doing something nobody asked for. The
    /// bindings a command made are the ones the replayed source makes again.
    fn replay(&mut self, lines: &[String]) -> (usize, usize) {
        let mut replayed = 0;
        let mut skipped = 0;
        for line in lines {
            if matches!(ReplCommand::parse(line), ReplCommand::NotACommand) {
                self.execute_line(line);
                replayed += 1;
            } else {
                skipped += 1;
            }
        }
        (replayed, skipped)
    }

    fn run_file(&self, path: &str) {
        match crate::run_file(path) {
            Ok(_) => println!("File '{}' executed successfully.", path),
            Err(e) => println!("Error running file: {}", e),
        }
    }

    fn debug_mode(&self) {
        let session = lock(&self.debug);
        let lines = session.breakpoints();
        println!(
            "Debugger: {} breakpoint(s) set. `:break <line>` sets one, `:next` steps \
             over a call, `:step` steps into it, `:continue` runs on.",
            lines.len()
        );
        for line in lines {
            println!("  line {}", line);
        }
    }

    /// Sets the breakpoint on `argument`, or clears one already set there.
    fn toggle_breakpoint(&self, argument: &str) {
        match line_number(argument) {
            Ok(line) => {
                let mut session = lock(&self.debug);
                if session.toggle_breakpoint(line) {
                    println!("Breakpoint set on line {}.", line);
                } else {
                    println!("Breakpoint removed from line {}.", line);
                }
            }
            Err(complaint) => println!("{}", complaint),
        }
    }

    /// Removes the breakpoint on `argument`, saying so when there was none.
    fn delete_breakpoint(&self, argument: &str) {
        match line_number(argument) {
            Ok(line) => {
                let mut session = lock(&self.debug);
                if session.clear_breakpoint(line) {
                    println!("Breakpoint removed from line {}.", line);
                } else {
                    println!("No breakpoint on line {}.", line);
                }
            }
            Err(complaint) => println!("{}", complaint),
        }
    }

    /// Lists where the breakpoints are, or that there are none.
    fn print_breakpoints(&self) {
        let session = lock(&self.debug);
        let lines = session.breakpoints();
        if lines.is_empty() {
            println!("No breakpoints set.");
            return;
        }
        println!("Breakpoints:");
        for line in lines {
            println!("  line {}", line);
        }
    }

    /// Arms a step for the next program the session runs.
    fn arm_step(&self, mode: StepMode) {
        lock(&self.debug).arm_step(mode);
        println!(
            "{}",
            match mode {
                StepMode::Into => "Stepping into the next call.",
                StepMode::Over => "Stepping over the next call.",
                StepMode::None => "Continuing.",
            }
        );
    }

    /// Says what a program typed here would do.
    ///
    /// `:continue` at the prompt cannot resume a program — nothing is running
    /// by then, because a stop is answered from inside the run — so this
    /// reports the state it would find rather than pretending to act.
    fn resume(&self) {
        let session = lock(&self.debug);
        if session.active() {
            println!("Running to the next breakpoint.");
        } else {
            println!("No breakpoints set and no step armed.");
        }
    }

    /// Prints the frame the program last stopped in, or says there was none.
    fn print_frame(&self) {
        let session = lock(&self.debug);
        let Some(frame) = session.last_stop() else {
            println!("Nothing has stopped yet.");
            return;
        };
        for line in frame.lines() {
            println!("{}", line);
        }
    }

    /// The `:inspect` listing for `name`, or `None` when nothing in the session is
    /// bound to it.
    ///
    /// The lookup asks the VM as well as the REPL's own map. `set`, `to` and
    /// `import` all bind in the VM, and the REPL's map held only `_`, so
    /// `:inspect <session-name>` answered "Variable not found" for every name the
    /// session had actually bound — including one it had bound a line earlier.
    ///
    /// The type is the Redblue one, from [`Value::type_name`]: the name `type_of`
    /// reports and a `Runtime` error quotes. It used to be
    /// `std::any::type_name_of_val`, which is the *Rust* type of every value the
    /// VM holds — so `:inspect counter` answered `Type: "redblue::value::Value"`
    /// for a number, a text, a list and a function alike, and told the user
    /// nothing about any of them.
    ///
    /// Built here rather than printed inline so the wording is something a test
    /// can read back.
    fn inspect_lines(&self, name: &str) -> Option<Vec<String>> {
        let value = self.lookup(name)?;
        Some(vec![
            format!("{}: {}", name, value),
            format!("Type: {}", value.type_name()),
        ])
    }

    /// Prints what [`Repl::inspect_lines`] has to say about `name`.
    fn inspect_value(&self, name: &str) {
        match self.inspect_lines(name) {
            Some(lines) => {
                for line in lines {
                    println!("{}", line);
                }
            }
            None => {
                println!("Variable '{}' not found.", name);
            }
        }
    }

    /// `:type <expression>` — the value, its kind, and where it was written.
    ///
    /// An *expression*, not a name: `.type [1, 2]` is a question about a value
    /// no session has ever bound, and `:inspect` could not ask it because it
    /// only ever looked a name up. The expression is evaluated through the
    /// session's own pipeline, so it is the same analyzer and the same bindings
    /// the next typed line would get — and a name nothing binds is that
    /// pipeline's ordinary error, printed rather than panicked.
    fn print_type(&mut self, code: &str) {
        match self.type_answer(code) {
            Ok(lines) => {
                for line in lines {
                    println!("{}", line);
                }
            }
            Err(message) => {
                // `paint_diagnostic`, not a whole-string paint: a failure that
                // arrives rendered carries the reader's own source line, and
                // that line is left as they typed it. A one-line refusal has no
                // gutter to be read from, so it is still painted whole.
                println!("{}", paint_diagnostic(&self.style, &message));
            }
        }
    }

    /// What `.type` has to say about `code`, or why it will not say it.
    ///
    /// `Err` rather than a line that reads like an answer: a name nothing binds
    /// is an unknown-variable error, and printing it inside a listing whose other
    /// lines are about values would make it look like a kind of value. The same
    /// channel carries the two refusals — a statement, and a call whose effects
    /// leave the value it answers — so a question that cannot be answered is
    /// reported as the question it could not answer.
    ///
    /// Three gates, in this order, and each of them is the reason the next one
    /// was needed:
    ///
    /// 1. **The program must be one expression.** [`introspect::expression_of`].
    /// 2. **No call may leave the value system.** [`Repl::effectful_call_in`].
    /// 3. **It runs on a fork.** [`Repl::evaluate_for_question`].
    ///
    /// The first two stop the world being changed; the third stops the *session*
    /// being changed, which is the half nothing else reaches: a `to` function's
    /// own `set` writes a global, and `uppercase` is a builtin rather than a
    /// statement.
    fn type_answer(&mut self, code: &str) -> Result<Vec<String>, String> {
        let program = parse_only(code).map_err(|error| failure_line(&self.style, code, &error))?;
        let expression = introspect::expression_of(&program)?;
        if let Some(call) = self.effectful_call_in(expression) {
            return Err(format!(
                "Error: :type is a question about a value, and {} does something outside \
                 the value it answers — a question cannot undo it. `:sighelp {}` describes it.",
                call, call
            ));
        }
        let value = self.evaluate_for_question(&program, code)?;
        let definition = self.definition_of_expression(code);
        Ok(introspect::type_lines(&value, definition.as_ref()))
    }

    /// The first call reachable from `expression` whose effects leave the value
    /// it answers, named the way the registry holds it.
    ///
    /// Read off the expression rather than out of a run, because the point is to
    /// refuse *before* the run: `files.delete("notes.rb")` has to be a message,
    /// not an answer about what `files.delete` returns. Every spelling is
    /// resolved to the registry's own name first — [`Vm::module_behind`] for an
    /// `import files as F`, [`stdlib::resolve`] for a module member written flat
    /// — so `files.delete(…)`, `files_delete(…)` and `F.delete(…)` are one
    /// refusal and not three holes.
    ///
    /// **And it follows the calls.** Reading only the expression finds the calls
    /// written in it, which is not the same set as the calls *made*: `to evil()
    /// files.delete("notes.rb") end` writes a call that appears nowhere in
    /// `:type evil()`, and a gate that only reads what it can see is a gate with
    /// one line written around it. So every call reached is resolved to the
    /// function it names — a bare name, a module member, an object's method
    /// ([`Vm::function_body`], [`Vm::object_method`]) — and that function's body
    /// is queued in its turn, transitively. A declaration that calls itself, or
    /// two that call each other, terminates because a body is only ever read
    /// once.
    ///
    /// What this cannot see is a call made *through a value* rather than through
    /// a name: `xs.map(evil)` never writes `evil` at all. That is not a gap in
    /// this walk, it is the reason [`Repl::evaluate_for_question`] also refuses
    /// at the point of the call — and the two are asked the same question of the
    /// same name by [`stdlib::is_effectful`], so neither can drift from the
    /// other.
    fn effectful_call_in(&self, expression: &Expr) -> Option<String> {
        let mut queue: VecDeque<introspect::Call> = introspect::calls_in(expression).into();
        let mut read: HashSet<String> = HashSet::new();

        while let Some(call) = queue.pop_front() {
            let module = call
                .receiver
                .as_deref()
                .map(|name| self.vm.module_behind(name));
            let name = call.builtin_name(module);
            if stdlib::is_effectful(&name) {
                return Some(name);
            }
            if !read.insert(name.clone()) {
                continue;
            }
            if let Some(body) = self.callee_body(&call, module) {
                queue.extend(introspect::calls_in_body(body));
            }
        }
        None
    }

    /// The statements behind `call`, when it names a function this session
    /// declared.
    ///
    /// The three shapes [`Vm::call`] and [`Vm::call_method`] can reach a
    /// declaration through, asked in the order they reach it. An object's method
    /// is not a bound name at all — `counter.bump()` is `objects[counter].methods[bump]`
    /// — so it is looked up from the receiver the call was written on and the
    /// name the two would build is the fallback rather than the first question.
    fn callee_body(&self, call: &introspect::Call, module: Option<&str>) -> Option<&[Stmt]> {
        let declared = match (module, &call.receiver) {
            (Some(_), Some(receiver)) => match self.vm.object_method(receiver, &call.member) {
                Some(Value::Function(function)) => Some(&function.body),
                _ => None,
            },
            _ => None,
        }
        .or_else(|| self.vm.function_body(&call.builtin_name(module)))?;

        match declared {
            FunctionBody::Statements(body) => Some(body),
            // A body compiled to bytecode has no statements to read. The tree-
            // walking VM builds `Statements` and the bytecode VM never reaches
            // `.type`, so this is unreachable in practice — and the walk is a
            // *refinement*, not the gate: [`Repl::evaluate_for_question`] refuses
            // the effect at the call itself whether or not this could see it.
            FunctionBody::Block { .. } => None,
        }
    }

    /// Evaluates `program` against the session's bindings and changes none of
    /// them.
    ///
    /// A [`Vm::fork`], not the session's own VM: the analyzer is seeded from the
    /// session exactly as a typed line would be — so `nosuchname` is that
    /// pipeline's own error and `nums[0]` resolves — and the run happens on a
    /// copy. A call that assigns, a builtin that prints, and the `_` binding
    /// `.type` never makes are all consequences of using the session's VM here,
    /// and none of them survives the fork.
    ///
    /// [`Vm::run_quietly`] rather than [`Vm::run`]: `say` inside a called
    /// function buffers its line, and a question is not a request to hear it.
    /// `print` is the one way out that is not buffered at all, so
    /// [`Vm::ask_questions`] is what keeps it off the terminal.
    ///
    /// And the fork is told it is answering a question ([`Vm::ask_questions`]),
    /// which refuses an effectful builtin *at the call* — the half of the gate
    /// that does not depend on reading the source. The fork shares the process
    /// with the session: the same root, the same filesystem, the same
    /// `RANDOM_STATE`, the same terminal. So a call no static reading of the
    /// expression could have found still has to be stopped where it happens.
    /// Three reach one that way — a `to` function's body, an object's method,
    /// and a function value handed to `map` — and none of them writes the name
    /// it calls into the expression. They are refused after the early gate has
    /// had its say, so the message the user reads is the early one wherever the
    /// early one can see it, and an error naming the offending call where it
    /// cannot.
    /// `code` is the question as the user typed it, and is what a failure is
    /// drawn against: a fork answers about the program in the command, so the
    /// line to quote is the command and not the history.
    fn evaluate_for_question(&self, program: &Program, code: &str) -> Result<Value, String> {
        analyzer::analyze_with_bound_names(program, &self.session_names())
            .map_err(|error| failure_line(&self.style, code, &error))?;
        let mut scratch = self.vm.fork();
        scratch.ask_questions(true);
        scratch
            .run_quietly(program)
            .map_err(|error| failure_line(&self.style, code, &error))
    }

    /// Where `:type`'s expression was written, when it is a bare name this
    /// session bound.
    ///
    /// A name is the only expression with a span to report: `[1, 2]` was
    /// written in the command itself, which the user is looking at, and pointing
    /// at the session's history index would be pointing at the question.
    fn definition_of_expression(&self, code: &str) -> Option<Definition> {
        let name = code.trim();
        if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return None;
        }
        self.definitions.get(name).cloned()
    }

    /// `:doc <module>` — what a module exposes, read out of the registry.
    ///
    /// A name that is not a module is refused by name. An empty listing would
    /// read like a module with nothing in it, which sends the user looking for a
    /// bug in the REPL rather than a typo in the command.
    fn print_module_doc(&self, module: &str) {
        match self.module_doc_answer(module.trim()) {
            Ok(lines) => {
                for line in lines {
                    println!("{}", line);
                }
            }
            Err(message) => {
                println!("{}", self.style.paint(Paint::Error, &message));
            }
        }
    }

    /// What `.doc` has to say about `module`, or the refusal that names the
    /// modules that do exist.
    fn module_doc_answer(&self, module: &str) -> Result<Vec<String>, String> {
        introspect::doc_lines(module).ok_or_else(|| {
            format!(
                "Error: '{}' is not a module. The modules are: {}",
                module,
                crate::stdlib::MODULES.join(", ")
            )
        })
    }

    /// `:which <name>` — builtin, this session, a module member, or nothing.
    fn print_which(&self, name: &str) {
        for line in self.which_answer(name) {
            println!("{}", line);
        }
    }

    fn which_answer(&self, name: &str) -> Vec<String> {
        let name = name.trim();
        introspect::which_lines(name, &self.origin_of(name))
    }

    /// The [`introspect::Origin`] of `name`, asking the VM the two facts only it
    /// knows: whether a bare name is bound, and whether a `Module.member`
    /// spelling is one this session published.
    ///
    /// Only a *recorded* line makes a session binding a span: a name the session
    /// holds but has no line for is reported by `:inspect`, which reads the value
    /// and does not care where it came from, and `.which` says the name is not
    /// one it can place rather than inventing a line 0.
    fn origin_of(&self, name: &str) -> introspect::Origin {
        let definition = self.definitions.get(name);
        let session_member = match name.split_once('.') {
            Some((module, member)) if !module.is_empty() && !member.is_empty() => {
                self.vm.published_member(&format!("{module}_{member}"))
            }
            _ => false,
        };
        introspect::classify(name, definition, session_member)
    }

    /// `:sighelp <name>` — the shape of a call.
    ///
    /// Nothing is invented for a name that has no shape: the refusal says the
    /// name is not something this language can call, which is the answer, and a
    /// guessed signature would be worse than none.
    fn print_sighelp(&self, name: &str) {
        match self.sighelp_answer(name.trim()) {
            Ok(lines) => {
                for line in lines {
                    println!("{}", line);
                }
            }
            Err(message) => {
                println!("{}", self.style.paint(Paint::Error, &message));
            }
        }
    }

    /// What `.sighelp` has to say about `name`, or the refusal for a name with no
    /// shape.
    ///
    /// The two refusals are different sentences because they are different
    /// mistakes: a keyword is a word the language already uses, and being told
    /// "no shape for `say`" reads as "`say` does not exist" when it answers every
    /// time.
    fn sighelp_answer(&self, name: &str) -> Result<Vec<String>, String> {
        introspect::sighelp_lines(name, self.lookup(name)).ok_or_else(|| {
            if Lexer::keywords().contains(&name) {
                format!(
                    "Error: '{}' is a keyword, not something to call: it is written {}.",
                    name, name
                )
            } else {
                format!(
                    "Error: no shape for '{}': it is not a builtin, and this session has not \
                     defined a function of that name.",
                    name
                )
            }
        })
    }

    /// The value bound to `name` anywhere in this session, VM included.
    fn lookup(&self, name: &str) -> Option<&Value> {
        self.variables
            .get(name)
            .or_else(|| self.vm.user_value(name))
    }

    /// `:time <expression> [n times]` — wall time, and the mean of the runs.
    fn print_time(&mut self, code: &str) {
        let answer = self.time_answer(code);
        self.print_measurement(answer);
    }

    /// `:bench <expression> [n times]` — min, median, max and mean of the runs.
    fn print_bench(&mut self, code: &str) {
        let answer = self.bench_answer(code);
        self.print_measurement(answer);
    }

    fn print_measurement(&self, answer: Result<Vec<String>, String>) {
        match answer {
            Ok(lines) => {
                for line in lines {
                    println!("{}", line);
                }
            }
            Err(message) => {
                // The same painting contract as the run path: a rendered
                // failure keeps the reader's own source line plain, and a
                // one-line refusal — `.time` asked for no expression at all —
                // has no gutter, so it is painted whole.
                println!("{}", paint_diagnostic(&self.style, &message));
            }
        }
    }

    /// What `.time` has to say about `code`, or why it will not say it.
    fn time_answer(&mut self, code: &str) -> Result<Vec<String>, String> {
        let measurement = measure::split_runs(code, "time", measure::TIME_RUNS)?;
        let samples = self.time_runs(&measurement)?;
        Ok(measure::time_lines(
            "time",
            measurement.expression,
            &samples,
        ))
    }

    /// What `.bench` has to say about `code`, or why it will not say it.
    fn bench_answer(&mut self, code: &str) -> Result<Vec<String>, String> {
        let measurement = measure::split_runs(code, "bench", measure::BENCH_RUNS)?;
        let samples = self.time_runs(&measurement)?;
        Ok(measure::bench_lines(
            "bench",
            measurement.expression,
            &samples,
        ))
    }

    /// Runs the measured program once per requested run, in this session's own
    /// VM, and answers with how long each of them took.
    ///
    /// **The session's VM, not a fork.** A measurement taken at the prompt is a
    /// question about the session as it stands: `.time x + 1` has to read the
    /// `x` the session bound, and what the timed program binds is still there
    /// afterwards. [`Repl::evaluate_for_question`] forks for the opposite reason
    /// — a question must change nothing — which is the wrong trade here, since
    /// measuring `set hits to hits + 1` a hundred times and learning nothing
    /// about the session is not a measurement.
    ///
    /// **Analysis once, before any run.** A program that cannot run is refused
    /// before the clock starts, so the error is the language's own rather than a
    /// per-run failure inside a loop.
    ///
    /// **A failure stops the loop.** The error is reported once, by name, and
    /// the measurement ends: timing the same failure N times is N copies of one
    /// line, and the side effects before it happened once rather than N times.
    fn time_runs(
        &mut self,
        measurement: &measure::Measurement<'_>,
    ) -> Result<Vec<Duration>, String> {
        let program = parse_only(measurement.expression)
            .map_err(|error| failure_line(&self.style, measurement.expression, &error))?;
        analyzer::analyze_with_bound_names(&program, &self.session_names())
            .map_err(|error| failure_line(&self.style, measurement.expression, &error))?;

        let mut samples = Vec::with_capacity(measurement.runs);
        for _ in 0..measurement.runs {
            let started = Instant::now();
            let outcome = self.vm.run_quietly(&program);
            let elapsed = started.elapsed();
            // What a run said is dropped, and the buffer drained every run: a
            // measurement that printed its result once per run is a flood, and
            // `.bench say 1` is a thing a user will type.
            self.vm.take_output();
            samples.push(elapsed);
            outcome.map_err(|error| failure_line(&self.style, measurement.expression, &error))?;
        }
        Ok(samples)
    }

    fn print_ast(&self, code: &str) {
        match Lexer::tokenize(code) {
            Ok(tokens) => {
                let mut parser = Parser::new(tokens);
                match parser.parse() {
                    Ok(program) => {
                        println!("{:#?}", program);
                    }
                    Err(e) => {
                        println!("Parse error: {}", e);
                    }
                }
            }
            Err(e) => {
                println!("Lexer error: {}", e);
            }
        }
    }

    fn print_tokens(&self, code: &str) {
        match Lexer::tokenize(code) {
            Ok(tokens) => {
                for token in tokens {
                    println!("{:?}", token);
                }
            }
            Err(e) => {
                println!("Lexer error: {}", e);
            }
        }
    }

    /// `:bytecode <expression>` — what the expression compiles to, one line per
    /// instruction.
    ///
    /// Compiled through the same frontend `rb compile` uses and rendered by the
    /// same disassembler, and handed to neither the session's VM nor the disk:
    /// the chunk exists only as the text just printed, because a listing is
    /// something to read.
    fn print_bytecode(&self, code: &str) {
        self.print_code_answer("bytecode", code, bytecode_view::bytecode_lines(code));
    }

    /// `:asm <expression>` — the same chunk, annotated: the constants table
    /// above it and a comment naming the source line on every instruction.
    fn print_asm(&self, code: &str) {
        self.print_code_answer("asm", code, bytecode_view::asm_lines(code));
    }

    /// Prints the header naming what was asked about, then the listing or the
    /// refusal.
    ///
    /// The header is the only thing here that is not the disassembler's own text,
    /// which is what lets the two be compared against `rb dis` byte for byte.
    fn print_code_answer(&self, command: &str, code: &str, answer: Result<Vec<String>, String>) {
        match answer {
            Ok(lines) => {
                println!("{command}: `{}`", code.trim());
                for line in lines {
                    println!("{}", line);
                }
            }
            Err(message) => {
                println!("{}", self.style.paint(Paint::Error, &message));
            }
        }
    }

    /// Prints [`EXAMPLES`], which is the same list the tests at the bottom of
    /// this module run. Each one is code the session has run and the analyzer has
    /// accepted, so `:example` cannot advertise a feature that is not there — the
    /// old examples promised `say "Hello, {{name}}!"`, and `say` printed
    /// `Hello, {{name}}!`.
    fn show_examples(&self) {
        println!("Example Redblue Code:");
        println!();
        for (caption, source) in EXAMPLES {
            println!("  {}", caption);
            for line in source.lines() {
                println!("  {}", line);
            }
            println!();
        }
    }
}

impl Default for Repl {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::io;
    use std::path::PathBuf;
    use std::rc::Rc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::{farewell, Repl, SessionEnd};
    use crate::repl::editor::{Line, LineSource};
    use crate::value::Value;

    /// Where the `.rb` file under test is written, inside the project checkout
    /// rather than in a user's temp directory.
    fn scratch_file(name: &str, source: &str) -> PathBuf {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let serial = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/tmp/repl-load");
        std::fs::create_dir_all(&dir).expect("scratch directory should be creatable");
        let path = dir.join(format!("{}-{}.rb", name, serial));
        std::fs::write(&path, source).expect("the file should be writable");
        path
    }

    /// Where a non-`.rb` file under test — a history file — is written.
    fn scratch_path(name: &str, extension: &str) -> PathBuf {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let serial = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/tmp/repl-session");
        std::fs::create_dir_all(&dir).expect("scratch directory should be creatable");
        let path = dir.join(format!("{}-{}.{}", name, serial, extension));
        std::fs::write(&path, "").expect("the file should be creatable");
        path
    }

    /// Feeds `lines` to the session one at a time, as the read loop does: each line
    /// is recorded in the history before it is dispatched, because that is the
    /// order the loop keeps them in.
    fn feed(repl: &mut Repl, lines: &[&str]) {
        for line in lines {
            repl.history.push((*line).to_string());
            if repl.handle_command(line) {
                continue;
            }
            repl.execute_line(line);
        }
    }

    /// Feeds `count` copies of `line` to a session, as a piped input would.
    fn feed_repeatedly(repl: &mut Repl, line: &str, count: usize) {
        for _ in 0..count {
            repl.history.push(line.to_string());
            repl.execute_line(line);
        }
    }

    /// A line source that answers with what the test tells it to, so the read
    /// loop can be driven through the parts a terminal owns — an interrupt, end
    /// of input, a failed read — without one.
    struct ScriptedSource {
        lines: std::collections::VecDeque<io::Result<Line>>,
        /// Every prompt the session asked at, in the order it asked.
        ///
        /// Recorded rather than discarded because the prompt is the whole subject
        /// of the continuation-numbering and `:prompt` tests: what a session
        /// prints at the user is only observable from the reader's side.
        prompts: Vec<String>,
    }

    impl ScriptedSource {
        fn new(lines: Vec<io::Result<Line>>) -> Self {
            Self {
                lines: lines.into_iter().collect(),
                prompts: Vec::new(),
            }
        }

        /// The prompts the session asked at, in order.
        fn prompts(&self) -> &[String] {
            &self.prompts
        }
    }

    impl LineSource for ScriptedSource {
        fn read_line(&mut self, prompt: &str) -> io::Result<Line> {
            self.prompts.push(prompt.to_string());
            self.lines.pop_front().unwrap_or(Ok(Line::Eof))
        }
    }

    /// Runs the read loop over `source`, the way [`Repl::run`] runs it over a
    /// terminal.
    fn drive(repl: &mut Repl, source: &mut ScriptedSource) -> SessionEnd {
        let names = Rc::new(RefCell::new(repl.session_names()));
        repl.session_loop(source, &names)
    }

    /// An editor with no names and no history file of its own, over `path`.
    fn editor_at(path: Option<PathBuf>) -> crate::repl::editor::EditSource {
        crate::repl::editor::EditSource::new(Rc::new(RefCell::new(Vec::new())), path)
            .expect("an editor that never reads a line should be creatable")
    }

    #[test]
    fn a_session_restores_what_a_previous_one_typed_and_not_the_history_files_header() {
        let path = scratch_path("restored", "history");
        std::fs::remove_file(&path).expect("the empty scratch file should be removable");

        let mut previous = editor_at(Some(path.clone()));
        previous.record("set restored_probe to 1");
        previous
            .save(&path)
            .expect("the history file should be writable");

        let restored = editor_at(Some(path));
        let mut repl = Repl::new();
        repl.restore_history(&restored);

        assert_eq!(
            repl.history.search("set restored_probe to 1"),
            Some("set restored_probe to 1".to_string()),
            "the line the previous session typed is in this session's history"
        );
        assert_eq!(
            repl.history.search("#"),
            None,
            "and the history file's own version header is not a line anyone typed"
        );
    }

    #[test]
    fn interrupt_abandons_the_line_and_keeps_the_session_alive() {
        let mut repl = Repl::new();
        let mut source = ScriptedSource::new(vec![
            Ok(Line::Text("set half_written to ".to_string())),
            Ok(Line::Interrupted),
            Ok(Line::Text("set after_interrupt to 7".to_string())),
            Ok(Line::Eof),
        ]);

        let end = drive(&mut repl, &mut source);

        assert_eq!(
            end,
            SessionEnd::Eof,
            "Ctrl-C does not end the session; the run after it still reached end of input"
        );
        assert!(
            repl.complete("after_interrupt")
                .contains(&"after_interrupt".to_string()),
            "the line typed after the interrupt ran"
        );
        assert!(
            !repl
                .complete("half_written")
                .contains(&"half_written".to_string()),
            "the line the interrupt abandoned did not run"
        );
        assert_eq!(repl.history.search("half_written"), None);
    }

    #[test]
    fn edge_control_d_ends_the_session_with_the_farewell_it_always_printed() {
        let mut repl = Repl::new();
        let mut source =
            ScriptedSource::new(vec![Ok(Line::Text("2 + 3".to_string())), Ok(Line::Eof)]);

        assert_eq!(
            drive(&mut repl, &mut source),
            SessionEnd::Eof,
            "end of input ends the session cleanly"
        );
        assert_eq!(
            farewell(SessionEnd::Eof),
            "Goodbye!",
            "and it is the farewell the session has always printed"
        );
        assert_eq!(farewell(SessionEnd::Quit), "Goodbye!");
        assert_eq!(
            farewell(SessionEnd::InputError),
            "Goodbye!",
            "even a terminal that goes away is a session that ends, not a panic"
        );
    }

    #[test]
    fn edge_a_read_that_fails_ends_the_session_and_is_reported_as_a_failure() {
        let mut repl = Repl::new();
        let mut source = ScriptedSource::new(vec![Err(io::Error::other("no terminal"))]);

        let end = drive(&mut repl, &mut source);

        assert_eq!(
            end,
            SessionEnd::InputError,
            "a failed read ends the session rather than being read as an empty line"
        );
        assert!(
            repl.running,
            "but it is not a quit: the session was not asked to stop"
        );
        assert_eq!(repl.history.len(), 0, "and nothing was recorded as typed");
    }

    #[test]
    fn edge_an_unclosed_block_is_still_reported_at_end_of_input() {
        let mut repl = Repl::new();
        let mut source = ScriptedSource::new(vec![
            Ok(Line::Text("if 1 = 1 then".to_string())),
            Ok(Line::Text("say \"inside\"".to_string())),
            Ok(Line::Eof),
        ]);

        assert_eq!(
            drive(&mut repl, &mut source),
            SessionEnd::Eof,
            "end of input still ends the session"
        );
        assert!(
            !repl.in_multiline,
            "and the block it was holding open is given up rather than left dangling"
        );
        assert!(
            repl.multiline_buffer.is_empty(),
            "the abandoned block is not held onto after the session ends"
        );
    }

    #[test]
    fn the_read_loop_trims_and_skips_a_blank_line_exactly_as_the_pipe_did() {
        let mut repl = Repl::new();
        let mut source = ScriptedSource::new(vec![
            Ok(Line::Text("   ".to_string())),
            Ok(Line::Text("  set padded to 5  ".to_string())),
            Ok(Line::Eof),
        ]);

        assert_eq!(drive(&mut repl, &mut source), SessionEnd::Eof);
        assert!(
            repl.complete("padded").contains(&"padded".to_string()),
            "the line runs with its surrounding whitespace trimmed"
        );
        assert_eq!(
            repl.history.get(0).map(String::as_str),
            Some("set padded to 5"),
            "and the history holds what was run, not what was typed around it"
        );
    }

    #[test]
    fn handle_command_consumes_a_command_and_leaves_source_alone() {
        let mut repl = Repl::new();

        assert!(
            !repl.handle_command("2 + 3"),
            "Redblue source is not a command and must fall through to the VM"
        );
        assert!(repl.handle_command(":help"));
        assert!(
            repl.handle_command(":bogus"),
            "an unrecognised command is consumed and reported, not run as source"
        );
    }

    #[test]
    fn handle_command_of_quit_stops_the_session() {
        let mut repl = Repl::new();

        assert!(repl.handle_command(":quit"));
        assert!(!repl.running, "quit ends the read-eval-print loop");
        assert!(
            repl.handle_command(":help"),
            "a later command is still handled rather than panicking on a dead session"
        );
    }

    #[test]
    fn handle_command_reports_a_command_missing_its_argument() {
        let mut repl = Repl::new();

        assert!(
            repl.handle_command(":load"),
            "a command with nothing to load is reported, not run with an empty path"
        );
        assert!(repl.running, "and it does not end the session");
    }

    #[test]
    fn a_session_records_every_line_it_reads() {
        let mut repl = Repl::new();

        // The read loop records each line before dispatching it, so the history
        // holds both commands and source, in the order they were typed.
        for line in ["set x to 1", ":vars", "2 + 3"] {
            repl.history.push(line.to_string());
        }

        assert_eq!(repl.history.len(), 3);
        assert_eq!(repl.history.get(2).map(String::as_str), Some("2 + 3"));
        assert_eq!(repl.history.search(":"), Some(":vars".to_string()));
    }

    #[test]
    fn edge_a_line_repeated_in_a_row_is_one_entry_of_history() {
        let mut repl = Repl::new();

        repl.history.push(":vars".to_string());
        repl.history.push(":vars".to_string());

        assert_eq!(repl.history.len(), 1);
    }

    /// An empty line is not a line the user ran, so it is not history.
    #[test]
    fn edge_empty_input_is_not_recorded_and_runs_nothing() {
        let mut repl = Repl::new();

        assert!(!repl.handle_command("   "));
        assert!(!repl.handle_command(""));

        assert_eq!(repl.history.len(), 0, "a blank line leaves no trace");
        assert!(repl.running);
    }

    #[test]
    fn complete_offers_the_names_the_session_has_bound() {
        let mut repl = Repl::new();
        repl.execute_code("set total to 3");

        let matches = repl.complete("to");
        assert!(
            matches.contains(&"total".to_string()),
            "a variable the session bound is completable: {:?}",
            matches
        );
        assert!(
            matches.contains(&"to".to_string()),
            "the keyword of the same name is offered too: {:?}",
            matches
        );
    }

    #[test]
    fn complete_offers_nothing_for_a_prefix_the_session_has_no_name_for() {
        let repl = Repl::new();

        assert!(
            repl.complete("zqx").is_empty(),
            "a prefix nothing starts with completes to nothing"
        );
    }

    #[test]
    fn edge_completion_after_reset_forgets_the_sessions_names() {
        let mut repl = Repl::new();
        repl.execute_code("set counter to 3");
        assert!(repl.complete("count").contains(&"counter".to_string()));

        repl.handle_command(":reset");

        assert!(
            !repl.complete("count").contains(&"counter".to_string()),
            "reset drops the bindings, so they cannot still be completed"
        );
    }

    /// A reset session is a session at a prompt. It used to replace the VM and
    /// clear the last result and leave the block buffer alone, so a `repeat` the
    /// user had opened but not closed was still half-typed after the reset: the
    /// next line was appended to it, and nothing ran until some `end` arrived that
    /// the user had no reason to type.
    #[test]
    fn edge_reset_drops_a_block_the_session_had_not_finished_typing() {
        let mut repl = Repl::new();

        feed(&mut repl, &["repeat 3 times"]);
        assert!(repl.in_multiline, "the block is open before the reset");

        assert!(repl.handle_command(":reset"));

        assert!(
            repl.multiline_buffer.is_empty(),
            "the buffer is gone: {:?}",
            repl.multiline_buffer
        );
        assert!(
            !repl.in_multiline,
            "and the session is back at a prompt rather than waiting for an end"
        );

        repl.execute_line(r#"say "hi""#);

        assert_eq!(
            repl.lookup("_"),
            None,
            "the line ran on its own, so the block it would have joined did not"
        );
    }

    /// `:help` says `:load <file>` "Load and run a file", so `:load` runs it and
    /// what the file declared becomes the session's own. It used to read the file
    /// and print its size, leaving the session holding nothing the file declared.
    #[test]
    fn load_runs_the_file_so_its_declarations_become_the_sessions_own() {
        let path = scratch_file("load-runs", "to shout(word)\n    say word\nend\n");
        let mut repl = Repl::new();

        assert!(repl.handle_command(&format!(":load {}", path.display())));
        assert!(
            repl.complete("shout").contains(&"shout".to_string()),
            "a function the loaded file declared is the session's to offer: {:?}",
            repl.complete("shout")
        );

        let _ = std::fs::remove_file(&path);
    }

    /// `:load` and `:run` differ in where the file's bindings land: `:load` binds
    /// them into this session, `:run` runs the file in a VM of its own. Before the
    /// fix neither did — `:load` read the file and ran nothing.
    #[test]
    fn load_keeps_a_value_the_file_bound_where_run_does_not() {
        let path = scratch_file("load-keeps", "set loaded_value to 7\n");

        let mut run_only = Repl::new();
        run_only.handle_command(&format!(":run {}", path.display()));
        assert!(
            !run_only
                .complete("loaded_v")
                .contains(&"loaded_value".to_string()),
            "a file run with :run binds into its own VM, not this session"
        );

        let mut loaded = Repl::new();
        loaded.handle_command(&format!(":load {}", path.display()));
        assert!(
            loaded
                .complete("loaded_v")
                .contains(&"loaded_value".to_string()),
            "a file loaded with :load binds into the session: {:?}",
            loaded.complete("loaded_v")
        );

        let _ = std::fs::remove_file(&path);
    }

    /// `:vars` is a listing, and a listing whose order changes from one session
    /// to the next for the same bindings cannot be read or compared. `variables`
    /// is a `HashMap`, so the names are sorted rather than printed in whatever
    /// order the map hands them over.
    #[test]
    fn edge_vars_lists_every_name_in_a_fixed_order() {
        let mut repl = Repl::new();

        assert_eq!(repl.variable_lines(), None, "a session with nothing bound");

        for name in ["zebra", "apple", "mango", "_"] {
            repl.variables.insert(name.to_string(), Value::Number(1.0));
        }

        assert_eq!(
            repl.variable_lines(),
            Some(vec![
                "  _ = 1".to_string(),
                "  apple = 1".to_string(),
                "  mango = 1".to_string(),
                "  zebra = 1".to_string(),
            ]),
            "the listing is in name order however the map was filled"
        );
    }

    /// A block opens continuation mode and closes it when it balances. These are
    /// the three openers the old last-word heuristic missed — `repeat 3 times`
    /// ends in `times`, `to greet(name)` in `)` and `for each i from 1 to 10` in
    /// `10` — so each of them used to run half-written and report a parse error.
    #[test]
    fn edge_every_block_form_asks_for_its_continuation_line() {
        for opener in [
            "if 1 is greater than 0 then",
            "repeat 3 times",
            "to greet(name)",
            "for each i from 1 to 10",
            "try",
            "while 1 is less than 10",
            "unless 1 is nothing then",
        ] {
            let mut repl = Repl::new();
            repl.execute_line(opener);

            assert!(
                repl.in_multiline,
                "{:?} opens a block, so the session keeps reading",
                opener
            );

            repl.execute_line("end");

            assert!(!repl.in_multiline, "{:?} is closed by its 'end'", opener);
            assert!(
                repl.multiline_buffer.is_empty(),
                "{:?} leaves nothing buffered once it has run",
                opener
            );
        }
    }

    /// A `to` binds into a local scope rather than into the globals, so scanning
    /// the globals alone never offered a function the session had just defined —
    /// the name a user is most likely to be reaching for next.
    #[test]
    fn complete_offers_a_function_the_session_defined() {
        let mut repl = Repl::new();

        feed(&mut repl, &["to shout(word)", "    say word", "end"]);

        let matches = repl.complete("shout");
        assert!(
            matches.contains(&"shout".to_string()),
            "a `to` the session defined is completable: {:?}",
            matches
        );
    }

    /// An `import` publishes a module's functions in the VM under the
    /// `MathUtils_circle_area` name a call through the module resolves to. That name
    /// is how the VM reaches the function, not one the program wrote, so it must not
    /// reach completion, `:vars` or `:funcs` — the module's own name is what a
    /// session offers instead. The call itself has to keep working, so this asserts
    /// both halves.
    #[test]
    fn edge_an_imported_modules_internal_names_are_not_session_names() {
        let mut repl = Repl::new();

        feed(&mut repl, &["import MathUtils"]);
        assert!(
            repl.run_code("MathUtils.circle_area(2)").is_ok(),
            "the name the module is reached by still calls through"
        );

        let names = repl.session_names();
        for name in &names {
            assert!(
                !name.contains("MathUtils_"),
                "{:?} is the VM's own name for a module member, not the session's: {:?}",
                name,
                names
            );
        }
        assert!(
            names.contains(&"MathUtils".to_string()) && names.contains(&"TAU".to_string()),
            "what the module binds under its own names is still the session's: {:?}",
            names
        );
        assert!(
            !repl
                .complete("MathUtils_")
                .contains(&"MathUtils_circle_area".to_string()),
            "and the internal name is not completable: {:?}",
            repl.complete("MathUtils_")
        );
        assert_eq!(
            repl.function_lines(),
            None,
            "a module's functions are reached as MathUtils.circle_area, so :funcs has \
         nothing of the session's own to list"
        );
    }

    /// A block the whole user typed in one line needs no continuation line, and a
    /// REPL that asked for one anyway would sit waiting for an `end` the user
    /// never typed.
    #[test]
    fn edge_a_block_typed_on_one_line_does_not_wait_for_an_end() {
        let mut repl = Repl::new();

        repl.execute_line("if 1 is greater than 0 then say \"yes\" end");

        assert!(
            !repl.in_multiline,
            "the block was already closed by the time the line ended"
        );
        assert!(repl.multiline_buffer.is_empty());
    }

    /// A stray `end` opens nothing. Matching the last word of a line made it open
    /// a block, and every line after it was buffered into that block and never run
    /// or reported.
    #[test]
    fn edge_a_stray_end_does_not_capture_the_lines_after_it() {
        let mut repl = Repl::new();

        repl.execute_line("end");
        assert!(
            !repl.in_multiline,
            "an 'end' that closes nothing opens nothing"
        );

        repl.execute_line("2 + 3");

        assert_eq!(
            repl.variables.get("_"),
            Some(&Value::Number(5.0)),
            "the line after a stray 'end' ran instead of being buffered"
        );
    }

    /// A block keyword inside a string is text. Matching the line's last word got
    /// this wrong too, in both directions.
    #[test]
    fn edge_a_block_keyword_inside_a_string_is_not_a_block() {
        let mut repl = Repl::new();

        repl.execute_line(r#"say "end""#);

        assert!(
            !repl.in_multiline,
            "a quoted 'end' neither closes nor opens a block"
        );
        assert!(repl.multiline_buffer.is_empty());
    }

    /// A line the lexer refuses is a line the user is going to retype. Waiting for
    /// an `end` that will never balance it would hang the session on it forever.
    #[test]
    fn edge_a_line_that_cannot_be_lexed_does_not_wait_for_an_end() {
        let mut repl = Repl::new();

        repl.execute_line("say \"unclosed");

        assert!(
            !repl.in_multiline,
            "an unlexable line is reported, not buffered forever"
        );
        assert!(repl.multiline_buffer.is_empty());
    }

    /// The two `to`s that are not function declarations are mid-statement. A
    /// detector that counted every `to` would hold the session open for an `end`
    /// the user has no reason to type, and a loop would then need a second one.
    #[test]
    fn edge_a_range_bound_or_an_alias_is_not_a_function_declaration() {
        let mut looped = Repl::new();
        looped.execute_line("for each i from 1 to 10");
        assert!(
            looped.in_multiline,
            "the loop itself is a block, whatever its range bound says"
        );
        looped.execute_line("end");
        assert!(
            !looped.in_multiline,
            "one 'end' closes the loop, so the range bound's 'to' is not a second block"
        );

        for line in ["import files to F", "set x to 10", r#"say "to""#] {
            let mut repl = Repl::new();
            repl.execute_line(line);

            assert!(
                !repl.in_multiline,
                "{:?} is a whole statement, not an unclosed body",
                line
            );
        }
    }

    /// EOF with a block still open ends the session, and says why. Dropping the
    /// buffer made an unfinished block indistinguishable from a finished one: the
    /// session printed `Goodbye!` either way.
    #[test]
    fn edge_eof_with_an_unclosed_block_reports_it_and_lets_the_buffer_go() {
        let mut repl = Repl::new();

        feed(
            &mut repl,
            &["if 1 is greater than 0 then", "    say \"yes\""],
        );
        assert!(repl.in_multiline);

        repl.report_unclosed_block();

        assert!(
            repl.multiline_buffer.is_empty(),
            "the unclosed lines are reported, not silently dropped"
        );
        assert!(
            !repl.in_multiline,
            "and the session is not left waiting for an end that will not come"
        );
    }

    /// Nothing buffered is nothing to report, so EOF after a complete program is
    /// a clean goodbye rather than a complaint about an empty block.
    #[test]
    fn edge_eof_with_nothing_buffered_is_not_an_incomplete_block() {
        let mut repl = Repl::new();

        repl.execute_line("2 + 3");
        repl.report_unclosed_block();

        assert!(!repl.in_multiline);
        assert!(repl.multiline_buffer.is_empty());
    }

    /// A REPL has to carry state from one line to the next, and it could not: the
    /// VM was reused so the *value* survived, but each line was analyzed against
    /// an empty scope, so `set x to 1` then `say x` was
    /// `AnalyzerError: Unknown variable 'x'`. Every other test here avoided a
    /// cross-line read, which is why the suite was green.
    #[test]
    fn a_name_bound_on_one_line_is_readable_by_the_next() {
        let mut repl = Repl::new();

        feed(
            &mut repl,
            &[
                "set x to 41",
                "set y to x + 1",
                "if y is greater than 40 then",
                "    say \"CARRYING\"",
                "end",
            ],
        );

        assert!(
            repl.complete("x").contains(&"x".to_string()),
            "the name is the session's: {:?}",
            repl.complete("x")
        );
        assert!(
            !repl.in_multiline,
            "the block that reads a bound name ran instead of being reported"
        );
    }

    /// The same across a `to`, which is the case a name list is most likely to
    /// miss: a function binds through `declare` + `set_var`, and a line that only
    /// calls it names nothing.
    #[test]
    fn a_function_defined_on_one_line_is_callable_from_the_next() {
        let mut repl = Repl::new();

        feed(
            &mut repl,
            &[
                "to shout(word)",
                "    say word",
                "end",
                r#"shout("CALLED")"#,
            ],
        );

        assert!(
            repl.complete("shout").contains(&"shout".to_string()),
            "the function is the session's: {:?}",
            repl.complete("shout")
        );
        assert_eq!(
            repl.lookup("shout").map(|value| value.to_string()),
            Some("<function shout>".to_string()),
            "and it is callable from the line that follows the declaration"
        );
    }

    /// `:funcs` is a listing. It used to print "User-defined functions are stored
    /// in the VM" and name none, so a test that ran it could not tell a working
    /// listing from a missing one.
    #[test]
    fn funcs_lists_the_functions_the_session_declared() {
        let mut repl = Repl::new();
        assert_eq!(repl.function_lines(), None, "a session with no functions");

        feed(
            &mut repl,
            &[
                "to shout(word)",
                "    say word",
                "end",
                "to whisper(word)",
                "    say word",
                "end",
            ],
        );

        assert_eq!(
            repl.function_lines(),
            Some(vec!["  shout".to_string(), "  whisper".to_string()]),
            "both functions, in name order"
        );
    }

    /// A variable a `set` bound is a name of the session, so it is in `:vars` and
    /// in completion. `:vars` read only the REPL's own map, which held `_` and
    /// nothing else, so a session that had bound a dozen names listed one.
    #[test]
    fn edge_vars_lists_the_names_the_session_has_bound() {
        let mut repl = Repl::new();

        feed(&mut repl, &["set zebra to 1", "set apple to 2"]);

        let lines = repl.variable_lines().expect("the session bound two names");
        assert!(
            lines.contains(&"  zebra = 1".to_string())
                && lines.contains(&"  apple = 2".to_string()),
            "a `set` the session made is one of its variables: {:?}",
            lines
        );
    }

    /// `:inspect` looks in the VM, not only in the REPL's own map. `set`, `to` and
    /// `import` all bind there, so it reported "Variable not found" for every name
    /// a session had actually bound — including one bound a line earlier.
    #[test]
    fn edge_inspect_finds_a_name_the_session_bound() {
        let mut repl = Repl::new();

        assert_eq!(repl.lookup("nothing_here"), None);

        feed(&mut repl, &["set counter to 7"]);

        assert_eq!(
            repl.lookup("counter").map(|value| value.to_string()),
            Some("7".to_string()),
            "the value a `set` bound is the session's to inspect"
        );
        assert!(
            repl.handle_command(":inspect counter"),
            "and the command that reads it runs"
        );
        assert_eq!(
            repl.lookup("nothing_here"),
            None,
            "a name nothing bound is still absent"
        );
    }

    /// `:inspect` reports the Redblue type of what it found, which is the name
    /// `type_of` reports and a `Runtime` error quotes. It used to report
    /// `std::any::type_name_of_val`, the *Rust* type of the value — so a number,
    /// a text, a list and a function were all `redblue::value::Value` and the
    /// line said the same thing about every value in the session.
    #[test]
    fn edge_inspect_reports_the_redblue_type_of_the_value() {
        let mut repl = Repl::new();

        feed(
            &mut repl,
            &[
                "set counter to 7",
                "set words to [\"a\", \"b\"]",
                "set who to \"ann\"",
                "set answer to yes",
                "to greet(who)",
                "    return who",
                "end",
            ],
        );

        for (name, expected) in [
            ("counter", "number"),
            ("words", "list"),
            ("who", "text"),
            ("answer", "yes/no"),
            ("greet", "function"),
        ] {
            let value = repl
                .lookup(name)
                .unwrap_or_else(|| panic!("the session bound {name:?}"));
            let lines = repl
                .inspect_lines(name)
                .unwrap_or_else(|| panic!("{name:?} is the session's to inspect"));
            assert_eq!(
                lines,
                vec![format!("{name}: {value}"), format!("Type: {expected}")],
                "{name:?} holds {value} and :inspect reports it as a {expected}"
            );
        }

        assert_eq!(
            repl.inspect_lines("nosuchthing"),
            None,
            "a name nothing bound has no lines to print"
        );
    }

    /// `:save` and `:restore` are the same file, and a restore has to bring back
    /// the *state*, not only the text of the session. It used to put the lines
    /// back in the history and nothing else, so `set x to 1`, `:save`, a fresh
    /// session and `:restore` left `say x` an `AnalyzerError: Unknown variable
    /// 'x'` — the lines were back and what they had bound was not.
    #[test]
    fn save_and_restore_round_trip_the_session_through_the_repl() {
        let path = scratch_path("session", "txt");
        let mut saved = Repl::new();
        feed(&mut saved, &["set x to 1", "say \"saved\""]);
        assert!(saved.handle_command(&format!(":save {}", path.display())));
        assert_eq!(saved.history.len(), 2, "both lines were recorded");

        let mut restored = Repl::new();
        assert!(restored.handle_command(&format!(":restore {}", path.display())));

        assert_eq!(
            restored.history.len(),
            2,
            "the restored session holds the lines that were saved"
        );
        assert_eq!(
            restored.history.get(0).map(String::as_str),
            Some("set x to 1")
        );
        assert_eq!(
            restored.history.search("say"),
            Some("say \"saved\"".to_string()),
            "and they are the lines the user can reach again"
        );
        assert_eq!(
            restored.lookup("x").map(|value| value.to_string()),
            Some("1".to_string()),
            "and the name they bound is the session's to read on the next line"
        );
        assert!(
            restored.run_code("say x").is_ok(),
            "so a restored session can say what it restored"
        );

        let _ = std::fs::remove_file(&path);
    }

    /// A restore runs the lines it read back, and a block the session had typed
    /// over several lines comes back whole — through the same buffering the read
    /// loop uses, not as one line of source.
    #[test]
    fn edge_a_restored_block_runs_as_the_block_it_was() {
        let path = scratch_path("block-session", "txt");
        let mut saved = Repl::new();
        feed(
            &mut saved,
            &["to shout(word)", "    say word", "end", "shout(\"SAVED\")"],
        );
        assert!(saved.handle_command(&format!(":save {}", path.display())));

        let mut restored = Repl::new();
        assert!(restored.handle_command(&format!(":restore {}", path.display())));

        assert!(
            restored.run_code("shout(\"RESTORED\")").is_ok(),
            "the function the saved lines declared is the restored session's to call"
        );
        assert!(
            !restored.in_multiline,
            "and the replayed block closed itself: {:?}",
            restored.multiline_buffer
        );

        let _ = std::fs::remove_file(&path);
    }

    /// A line of the saved file that was a command is listed again and not run
    /// again. `:restore` used to end the session it was restoring into, because
    /// the saved file held the `:quit` that wrote it — a command that quits is
    /// not something a restore replays.
    #[test]
    fn edge_a_restored_command_is_listed_and_not_run_again() {
        let path = scratch_path("command-session", "txt");
        let mut saved = Repl::new();
        feed(&mut saved, &["set x to 1", ":vars"]);
        assert!(saved.handle_command(&format!(":save {}", path.display())));

        let mut restored = Repl::new();
        assert!(restored.handle_command(&format!(":restore {}", path.display())));

        assert!(
            restored.running,
            "the session is still there after the restore"
        );
        assert_eq!(
            restored.history.len(),
            2,
            "both lines are back, the command among them"
        );
        assert_eq!(
            restored.lookup("x").map(|value| value.to_string()),
            Some("1".to_string()),
            "and the source among them ran, which is the binding it made"
        );

        let _ = std::fs::remove_file(&path);
    }

    /// A restore of a file that is not there is a failure the user is told about,
    /// and the history it was going to replace is left alone.
    #[test]
    fn edge_restore_of_a_missing_file_leaves_the_history_alone() {
        let path = scratch_path("never-saved", "txt");
        let _ = std::fs::remove_file(&path);
        assert!(
            !path.exists(),
            "the path under test must not exist for this to mean anything"
        );

        let mut repl = Repl::new();
        feed(&mut repl, &["set x to 1"]);

        assert!(repl.handle_command(&format!(":restore {}", path.display())));

        assert_eq!(
            repl.history.len(),
            1,
            "a restore that failed did not throw the session's own history away"
        );
    }

    /// A block the user opened and never closed buffered without limit, so a piped
    /// block whose `end` never arrived grew until the process ran out of memory —
    /// and `report_unclosed_block` then printed all of it. Past the limit the
    /// session drops the block, says so, and goes back to reading lines.
    #[test]
    fn edge_a_block_that_outgrows_the_buffer_is_dropped_not_kept() {
        let mut repl = Repl::new();
        repl.execute_line("repeat 3 times");

        feed_repeatedly(&mut repl, r#"    say "x""#, super::MAX_BLOCK_LINES + 10);

        assert!(
            repl.multiline_buffer.is_empty(),
            "the block is dropped rather than buffered without limit"
        );
        assert!(
            !repl.in_multiline,
            "and the session goes back to reading whole lines"
        );

        repl.execute_line("2 + 3");
        assert_eq!(
            repl.lookup("_").map(|value| value.to_string()),
            Some("5".to_string()),
            "the session carries on after giving up on the block"
        );
    }

    /// A block can be huge in a handful of lines, so the byte limit is not implied
    /// by the line limit.
    #[test]
    fn edge_a_block_of_few_very_long_lines_is_dropped_too() {
        let mut repl = Repl::new();
        repl.execute_line("repeat 3 times");

        let long = format!("say \"{}\"", "a".repeat(super::MAX_BLOCK_BYTES / 4));
        feed_repeatedly(&mut repl, &long, 8);

        assert!(
            repl.multiline_buffer.is_empty(),
            "{} bytes of block is more than a session holds",
            super::MAX_BLOCK_BYTES
        );
        assert!(!repl.in_multiline);
    }

    /// The report of an unclosed block echoes the lines back so the user can see
    /// what was lost — but not all of them, or the report is the second problem.
    /// The count of what was left out is printed instead.
    #[test]
    fn edge_the_report_of_an_unclosed_block_echoes_only_some_of_it() {
        let mut repl = Repl::new();
        repl.execute_line("repeat 3 times");

        let body = super::BLOCK_REPORT_LINES + 5;
        let lines: Vec<String> = (0..body)
            .map(|index| format!("    say \"{}\"", index))
            .collect();
        feed(
            &mut repl,
            &lines.iter().map(String::as_str).collect::<Vec<_>>(),
        );

        repl.report_unclosed_block();

        assert!(
            repl.multiline_buffer.is_empty(),
            "and the buffer is released either way"
        );
        assert!(
            repl.history.len() <= body + 1,
            "the session kept the lines it read"
        );
    }

    /// Everything `:help` shows under "Quick Reference" is a program this session
    /// can run. It was a hand-written list, and `if x > 5 then` on it is not
    /// Redblue — a user who typed what the help showed got a parse error.
    #[test]
    fn edge_every_quick_reference_snippet_is_a_program_the_session_runs() {
        let mut repl = Repl::new();

        for (caption, source) in super::QUICK_REFERENCE {
            let outcome = repl.run_code(source);
            assert!(
                outcome.is_ok(),
                "the {:?} example {:?} is Redblue: {:?}",
                caption,
                source,
                outcome.err()
            );
        }
    }

    /// `:example` is the same list, and it used to promise an interpolation `say`
    /// does not perform: `say "Hello, {{name}}!"` printed `Hello, {{name}}!`. Each
    /// entry is run here, so an example cannot be shown unless the session can run
    /// it.
    #[test]
    fn edge_every_example_is_a_program_the_session_runs() {
        let mut repl = Repl::new();

        for (caption, source) in super::EXAMPLES {
            let outcome = repl.run_code(source);
            assert!(
                outcome.is_ok(),
                "the {:?} example is Redblue: {:?}",
                caption,
                outcome.err()
            );
            assert!(
                !source.contains("{{") && !source.contains("}}"),
                "the {:?} example does not promise an interpolation `say` does not do",
                caption
            );
        }
    }

    /// The four questions about code, answered by a live session rather than by
    /// the pure helpers they are written on.
    ///
    /// They are driven through [`Repl::feed`] — history, dispatch and all — so a
    /// command that parses but is never dispatched fails here. Before this phase
    /// the table had seventeen commands and not one that inspected anything:
    /// answering "what kind of thing is `[1, 2]`" or "where does `uppercase`
    /// come from" meant leaving the REPL.
    mod introspection {
        use super::{feed, Repl};
        use crate::repl::commands::ReplCommand;
        use crate::stdlib;

        /// A session with three bindings, fed through the read loop's order.
        fn session_with_bindings() -> Repl {
            let mut repl = Repl::new();
            feed(
                &mut repl,
                &[
                    "set counter to 3",
                    "set nums to [1, 2]",
                    "to double(n)\n    return n * 2\nend",
                ],
            );
            repl
        }

        /// A directory for a test's files, named after the test so two running in
        /// parallel cannot collide. Under `target/tmp/`, never `/tmp`.
        fn temp_dir(name: &str) -> std::path::PathBuf {
            let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("target/tmp")
                .join(name);
            std::fs::create_dir_all(&dir)
                .unwrap_or_else(|e| panic!("cannot create {}: {e}", dir.display()));
            dir
        }

        /// `.type` answers with the value, its kind, and where it was written —
        /// the third of which nothing in the REPL said before.
        #[test]
        fn type_reports_the_value_its_kind_and_the_line_it_was_written_on() {
            let mut repl = session_with_bindings();

            let lines = repl
                .type_answer("nums")
                .expect("a bound name is a question :type can answer");
            let joined = lines.join("\n");

            assert!(
                joined.contains("value: [1, 2]"),
                "the value as the language prints it: {:?}",
                lines
            );
            assert!(
                joined.contains("kind:  list (2 item(s))"),
                "the kind and how much of it: {:?}",
                lines
            );
            assert!(
                joined.contains("from:  <session>:2"),
                "and the line it was written on, which is the line :history shows: {:?}",
                lines
            );
            assert!(
                joined.contains("set nums to [1, 2]"),
                "quoted back, so the span can be checked against the history: {:?}",
                lines
            );
        }

        /// A question is a question. `.type` used to run whatever it was given
        /// through the session's own pipeline, so `:type set sneaky to 1` bound a
        /// name the session's history holds no line for — `.which` would then
        /// call it unknown while the next typed line reached it — and `:type say
        /// "hi"` printed a line nobody asked for. Both are refused by name, and
        /// the session is left exactly as the question found it.
        #[test]
        fn edge_type_refuses_a_program_and_the_session_never_knows_it_happened() {
            let mut repl = session_with_bindings();
            let last_result = repl.lookup("_").cloned();

            for (program, keyword) in [
                ("set sneaky to 1", "set"),
                ("say \"typed, not said\"", "say"),
                ("print \"typed, not printed\"", "print"),
                ("import files", "import"),
                ("to helper()\n    return 1\nend", "to"),
                ("set a to 1\nset b to 2", "statement"),
            ] {
                let refusal = repl
                    .type_answer(program)
                    .expect_err("a program is not a question about one value");
                assert!(
                    refusal.contains(keyword),
                    "{:?} is refused naming {:?}, got: {}",
                    program,
                    keyword,
                    refusal
                );
                assert!(
                    refusal.starts_with("Error:"),
                    "and the refusal is an error, not a value of some kind: {}",
                    refusal
                );
            }

            assert!(
                repl.lookup("sneaky").is_none() && repl.lookup("helper").is_none(),
                "no name was bound by a command that refused to run"
            );
            assert_eq!(
                repl.which_answer("sneaky").join("\n"),
                "sneaky: unknown",
                "and nothing is left behind for :which to be wrong about"
            );
            assert_eq!(
                repl.lookup("_").cloned(),
                last_result,
                "and the last result is still the last line's, not a question's"
            );
            assert!(
                repl.type_answer("counter").is_ok(),
                "the session is still a session: it answers the next question"
            );
        }

        /// The one thing a clone of the bindings cannot undo is an effect that
        /// leaves the process, so a call that reaches the filesystem, the
        /// network, the clock, the terminal, standard input or the random stream
        /// is refused before it runs. Asserted on the file, because "it said no"
        /// and "the file is still there" are different claims and only one of
        /// them is the point.
        ///
        /// A *read* is in that list now, and it was not. `files.read` changes
        /// nothing on disk, so it was treated as answerable — which is how a
        /// question could block forever on a FIFO and how `files.read("/home/…")`
        /// printed the user's home directory into a transcript they had agreed to
        /// nothing about. It blocks, and it reaches out of the process to do it.
        #[test]
        fn edge_type_refuses_a_call_that_changes_the_world_and_the_file_survives() {
            let dir = temp_dir("repl_type_effects");
            let path = dir.join("notes.rb");
            let copy = dir.join("copy.rb");
            std::fs::write(&path, "original").expect("the test needs a file to protect");
            let _ = std::fs::remove_file(&copy);
            let quoted = path.display().to_string();

            let mut repl = Repl::new();
            for call in [
                format!("files.write(\"{quoted}\", \"clobbered\")"),
                format!("files.delete(\"{quoted}\")"),
                format!("files.copy(\"{quoted}\", \"{}\")", copy.display()),
                format!("files_append(\"{quoted}\", \"more\")"),
                format!("files.read(\"{quoted}\")"),
                format!("files.lines(\"{quoted}\")"),
                format!("files.exists(\"{quoted}\")"),
                "time.sleep(0)".to_string(),
                "time_sleep(0)".to_string(),
                "time.now()".to_string(),
                "time_now()".to_string(),
                "input()".to_string(),
                "console.log(\"typed\")".to_string(),
                "random.seed(1)".to_string(),
                "random()".to_string(),
                "random.number()".to_string(),
                "random.choice([1, 2])".to_string(),
                "random.shuffle([1, 2])".to_string(),
            ] {
                let refusal = repl
                    .type_answer(&call)
                    .expect_err("a call that leaves the process is not a question");
                assert!(
                    refusal.starts_with("Error:") && refusal.contains("sighelp"),
                    "{} is refused with somewhere to go instead: {}",
                    call,
                    refusal
                );
            }

            assert_eq!(
                std::fs::read_to_string(&path).expect("the file is still there"),
                "original",
                "not one of those calls wrote to it"
            );
            assert!(
                !copy.exists(),
                "and the copy was never made either: a refusal is not a plan"
            );

            // And the value system itself is still answerable — the refusal list
            // is a list of the calls that leave it, not a refusal to answer.
            let lines = repl
                .type_answer("uppercase(\"redblue\")")
                .expect("a call that only answers a value is still a question");
            assert!(
                lines[0].contains("value: REDBLUE"),
                "what does not leave the process still answers: {:?}",
                lines
            );
        }

        /// An alias is another name for a module, so a call through one reaches
        /// the same function and has to be refused the same way — and a call
        /// through an alias to a function that *is* answerable still answers, so
        /// the rule is about what the function does and not about how it is
        /// written.
        #[test]
        fn edge_type_refuses_a_call_written_through_an_import_alias() {
            let mut repl = Repl::new();
            feed(&mut repl, &["import files as F", "import json to J"]);

            for (call, named) in [
                ("F.delete(\"notes.rb\")", Some("files_delete")),
                // The reads are refused through an alias exactly as they are
                // under the module's own name: `F` *is* `files`.
                ("F.read(\"notes.rb\")", Some("files_read")),
                ("F.exists(\"notes.rb\")", Some("files_exists")),
                ("J.parse(\"{}\")", None),
                ("J.stringify([1])", None),
            ] {
                match named {
                    // The refusal names the registry's function, not the spelling
                    // the user typed: an alias is a name for a module, not a
                    // second module.
                    Some(expected) => {
                        let refusal = repl
                            .type_answer(call)
                            .expect_err("a call behind an alias is the same call");
                        assert!(
                            refusal.contains(expected),
                            "{} is refused as {}, got: {}",
                            call,
                            expected,
                            refusal
                        );
                    }
                    None => {
                        assert!(
                            repl.type_answer(call).is_ok(),
                            "{} changes nothing outside the value it answers",
                            call
                        );
                    }
                }
            }
        }

        /// A call to a `to` function is a real question with a real answer, and
        /// the function's own `set` writes a global. So the run happens on a fork
        /// of the session's bindings: `counter` is still 3 afterwards, because
        /// describing what `bump()` answers is not the same as running it.
        #[test]
        fn type_calls_a_session_function_without_running_it_against_the_session() {
            let mut repl = session_with_bindings();
            feed(
                &mut repl,
                &["to bump()\n    set counter to counter + 1\n    say \"bumped\"\n    return counter\nend"],
            );
            let before = repl.lookup("counter").cloned().expect("counter is bound");
            let recorded_before = repl.which_answer("counter").join("\n");

            let lines = repl
                .type_answer("bump()")
                .expect("a function the session declared is a call it can describe");
            assert!(
                lines[0].contains("value: 4"),
                "what it answers is the value after the increment: {:?}",
                lines
            );

            assert_eq!(
                repl.lookup("counter"),
                Some(&before),
                "and the session's own counter is untouched: the call ran on a fork"
            );
            assert_eq!(
                repl.which_answer("counter").join("\n"),
                recorded_before,
                "and nothing else moved either — asking a question changes no answer \
                 the next question would give"
            );
        }

        /// A question about a value cannot answer one about a *call the session made
        /// before it*, and reading only the expression the user typed is how that
        /// is got wrong.
        ///
        /// `to evil()` calls `files.delete`, and `:type evil()` writes no call at
        /// all: the only thing in it is the name `evil`. So the early gate read
        /// the expression, found nothing effectful, and ran the question — on a
        /// fork, which copies the bindings and shares the process — and the file
        /// was deleted. The fork was never going to help with this and never was
        /// asked to; the hole was in *what was refused*, not in what was isolated.
        ///
        /// Every shape a declaration can hide behind is checked, because each one
        /// is a separate way to spell the same mistake: through another function
        /// (so the walk has to be transitive, not one level), through a module's
        /// own member, through an object's method, and through a function *value*
        /// handed to `map` — which writes no name at all, and is therefore only
        /// stoppable at the call.
        ///
        /// The file is the witness for every one of them. A refusal that said
        /// "no" while the file went anyway would pass an assertion on the
        /// message alone, so nothing here checks the message until afterwards.
        #[test]
        fn edge_type_refuses_a_question_whose_call_hides_behind_a_declared_function() {
            let dir = temp_dir("repl_type_callee");
            let path = dir.join("victim.rb");
            std::fs::write(&path, "original").expect("the test needs a file to protect");
            let quoted = path.display().to_string();

            let mut repl = Repl::new();
            feed(
                &mut repl,
                &[
                    // Directly.
                    &format!("to evil()\n    files.delete(\"{quoted}\")\nend"),
                    // One call removed: the walk has to follow the body.
                    "to middle()\n    evil()\nend",
                    // Two removed, and mutual: the walk has to terminate.
                    "to left()\n    right()\nend",
                    &format!("to right()\n    left()\n    files.delete(\"{quoted}\")\nend"),
                    // Through a module the session declared itself.
                    &format!(
                        "module Vault\n    export empty\n    to empty()\n        \
                         files.delete(\"{quoted}\")\n    end\nend"
                    ),
                    // Through an object's method, where the callee is not a name
                    // any binding holds.
                    &format!(
                        "object Safe\n    to empty()\n        files.delete(\"{quoted}\")\n    end\nend"
                    ),
                ],
            );

            for question in [
                "evil()",
                "middle()",
                "left()",
                "Vault.empty()",
                "Safe.empty()",
            ] {
                let refusal = repl
                    .type_answer(question)
                    .expect_err("a question whose call reaches the filesystem is not a question");
                // The *early* refusal, not merely a refusal: these are all calls
                // the walk can reach by name, and each is answered before the run
                // with somewhere to go instead. A refusal raised at the call
                // would say the same thing about the file and say nothing about
                // when it was decided.
                assert!(
                    refusal.contains("`:sighelp files_delete` describes it"),
                    "{question} is refused by name, before anything runs: {refusal}"
                );
                assert_eq!(
                    std::fs::read_to_string(&path).expect("the file is still there"),
                    "original",
                    "{question} deleted the file it was a question about"
                );
            }

            // And the one that only a refusal *at the call* can stop, because the
            // expression names nothing that is called: `map` is handed a function
            // value, and the call to it is made by the builtin rather than
            // written in the source.
            feed(&mut repl, &["to keep(n)\n    return n\nend"]);
            let refusal = repl
                .type_answer("map([1, 2], evil)")
                .expect_err("a function value handed to map is called just the same");
            assert!(
                refusal.contains("files_delete") && !refusal.contains(":sighelp"),
                "this one is caught where it happens rather than where it is written: {refusal}"
            );
            assert_eq!(
                std::fs::read_to_string(&path).expect("the file is still there"),
                "original",
                "the file is still there after the last of them"
            );

            // The walk is a refinement, not the whole rule: a function that only
            // answers a value is still describable, through the same door.
            let lines = repl
                .type_answer("map([1, 2], keep)")
                .expect("mapping a function that only answers is a question with an answer");
            assert!(
                lines[0].contains("value: [1, 2]"),
                "and it answers: {:?}",
                lines
            );
        }

        /// The refusal belongs to the *question*, and to nothing else.
        ///
        /// It is a flag on one VM rather than a change to the language, so the
        /// sharpest way to pin it is to show that the very call `.type` refuses
        /// still works on the line after: `files.write` is what the file module is
        /// for, and a gate that had leaked into ordinary evaluation would have
        /// broken the language to protect it. A file written by the session and a
        /// question about it refused is the only pair of claims that can both be
        /// true.
        #[test]
        fn edge_the_refusal_is_the_questions_and_not_the_language() {
            let dir = temp_dir("repl_type_only_questions");
            let path = dir.join("notes.rb");
            std::fs::write(&path, "before").expect("the test needs a file");
            let quoted = path.display().to_string();

            let mut repl = Repl::new();

            // A session line runs on the session's own VM and is not guarded.
            let write = format!("files.write(\"{quoted}\", \"after\")");
            feed(&mut repl, &[&write]);
            assert_eq!(
                std::fs::read_to_string(&path).expect("the file is still there"),
                "after",
                "a program still writes files: the guard is a question's, not the language's"
            );

            // And the same call, asked as a question, is refused.
            let refusal = repl
                .type_answer(&format!("files.write(\"{quoted}\", \"clobbered\")"))
                .expect_err("asking the question is not running the line");
            assert!(
                refusal.contains("files_write"),
                "and it is refused as what it would have done: {refusal}"
            );
            assert_eq!(
                std::fs::read_to_string(&path).expect("the file is still there"),
                "after",
                "the refused question wrote nothing over it"
            );
        }

        /// A deep tree is a diagnostic, not a crash.
        ///
        /// `.type` walks the expression it is given before it evaluates it, and
        /// that walk used to recurse — on the REPL's own thread, which is the
        /// thread the user is typing on and is not the sized one
        /// [`crate::interpreter::run_isolated`] spawns for a program. The parser
        /// bounds nesting, so a tree deep enough to overflow is refused before
        /// the walk sees it; that is the *second* line of defence and this is what
        /// pins it, because the first one is the walk itself and it no longer
        /// exists.
        #[test]
        fn edge_a_deeply_nested_question_is_refused_rather_than_overflowing() {
            let mut repl = Repl::new();

            for question in [
                format!("{}1{}", "a+(".repeat(4000), ")".repeat(4000)),
                format!("{}b", "a.".repeat(4000)),
                format!("{}1{}", "[".repeat(4000), "]".repeat(4000)),
            ] {
                let refusal = repl
                    .type_answer(&question)
                    .expect_err("a tree deeper than the parser allows has no answer");
                assert!(
                    refusal.starts_with("Error:"),
                    "and it is reported: {}",
                    &refusal[..refusal.len().min(120)]
                );
            }

            // Premise: the session is still answering, so nothing above overflowed
            // anything and took the process with it.
            assert!(
                repl.type_answer("uppercase(\"ok\")")
                    .expect("the session survived the deep questions")
                    .join("")
                    .contains("OK"),
                "and a question afterwards is answered normally"
            );
        }

        /// `:reset` forgets the bindings. It used to keep the lines they were
        /// bound on, so `.which x` reported `<session>:3` — quoting a line of a
        /// session that had been thrown away — for a name nothing held any more.
        #[test]
        fn edge_a_reset_forgets_the_lines_of_the_bindings_it_forgets() {
            let mut repl = session_with_bindings();
            assert!(
                repl.which_answer("counter")
                    .contains(&"counter: session binding".to_string()),
                "premise: the binding is reported before the reset"
            );

            repl.reset_session();

            let answer = repl.which_answer("counter").join("\n");
            assert_eq!(answer, "counter: unknown", "a reset session holds nothing");
            assert!(
                !answer.contains("<session>"),
                "and there is no line left to point at: {}",
                answer
            );
            assert!(
                repl.type_answer("counter").is_err(),
                "and the value is gone with the binding"
            );
        }

        /// One `import` binds three names here — two modules and an alias — and
        /// all three are recorded. Reading only the identifier after the keyword
        /// recorded `files` and called `network` and `net` unknown, for names the
        /// session held and a program would have called.
        #[test]
        fn edge_which_names_every_name_one_import_binds() {
            let mut repl = Repl::new();
            feed(&mut repl, &["import files, network as net"]);

            for name in ["files", "network", "net"] {
                let answer = repl.which_answer(name).join("\n");
                assert!(
                    answer.contains("session binding"),
                    "{} is bound by the import that reached it: {}",
                    name,
                    answer
                );
                assert!(
                    answer.contains("<session>:1"),
                    "on the line it was written on: {}",
                    answer
                );
                assert!(
                    answer.contains("import files, network as net"),
                    "which is quoted so the span can be checked: {}",
                    answer
                );
            }

            // The older alias spelling binds the same two kinds of name.
            feed(&mut repl, &["import csv as C"]);
            for name in ["csv", "C"] {
                assert!(
                    repl.which_answer(name)
                        .contains(&format!("{name}: session binding")),
                    "{} is bound by `import csv as C` too",
                    name
                );
            }
        }

        /// A `module Foo … end` declares a namespace rather than binding a
        /// value, which is why "was the name bound?" is the wrong question for it.
        /// `.which Foo` said *unknown* for a declaration the session had just
        /// run, and the documented contract said it should be recorded.
        #[test]
        fn edge_which_names_a_module_the_session_declared_itself() {
            let mut repl = Repl::new();
            feed(
                &mut repl,
                &["module Greeter\n    to greet(n)\n        return \"hi, \" + n\n    end\n    export all\nend"],
            );

            let answer = repl.which_answer("Greeter").join("\n");
            assert!(
                answer.contains("session binding"),
                "the declaration is a binding of a namespace: {}",
                answer
            );
            assert!(
                answer.contains("<session>:1") && answer.contains("module Greeter"),
                "reported where it was written, and quoted: {}",
                answer
            );

            let member = repl.which_answer("Greeter.greet").join("\n");
            assert!(
                member.contains("session module member") && member.contains("of module Greeter"),
                "and what it published is the session's own: {}",
                member
            );
        }

        /// `set x to y` reads `y` and writes `x`. The `to` in it looks exactly
        /// like the `to` that opens a declaration, and treating it as one moved
        /// `y` to a line on which it was never written — the precise failure
        /// reading the source rather than a before/after diff was meant to avoid.
        #[test]
        fn edge_a_value_read_by_a_set_stays_on_the_line_that_bound_it() {
            let mut repl = Repl::new();
            feed(&mut repl, &["set y to 1", "set x to y"]);

            let read = repl.which_answer("y").join("\n");
            assert!(
                read.contains("<session>:1") && read.contains("set y to 1"),
                "y was bound on the first line and only read on the second: {}",
                read
            );
            let written = repl.which_answer("x").join("\n");
            assert!(
                written.contains("<session>:2"),
                "and x is on the line that wrote it: {}",
                written
            );
        }

        /// An expression nobody bound is still a question `.type` answers, and
        /// the span it reports is the command itself rather than a history line
        /// that has nothing to do with the value.
        #[test]
        fn type_answers_about_an_expression_the_session_never_bound() {
            let mut repl = session_with_bindings();

            let lines = repl
                .type_answer("[1, 2]")
                .expect("a literal is an expression, not a name to look up");
            let joined = lines.join("\n");

            assert!(joined.contains("value: [1, 2]"), "{:?}", lines);
            assert!(
                joined.contains("evaluated at the prompt"),
                "a literal was written in the command, so no other span is true: {:?}",
                lines
            );
        }

        /// A `.type` over something the session cannot evaluate is *that* error,
        /// not a panic and not a value of some kind: a number added to text, an
        /// unterminated string, and a division by zero each come back through the
        /// same channel a typed line would print them through — and the session is
        /// still a session afterwards.
        #[test]
        fn edge_type_reports_the_sessions_own_errors_and_survives_them() {
            let mut repl = session_with_bindings();

            for (expression, needle) in [
                ("1 + \"a\"", "Cannot add non-numbers"),
                ("\"abc", "Unterminated string"),
                ("1 / 0", "Division by zero"),
                // One past the end, on a value nobody has indexed before: `.type`
                // evaluates the expression, so an out-of-bounds index reaches the
                // session's own bounds check and has to come back as its message.
                ("[1][9]", "Index 9 is out of bounds"),
            ] {
                let error = repl.type_answer(expression).unwrap_err();
                assert!(
                    error.contains(needle),
                    "{:?} fails with {:?}, got {:?}",
                    expression,
                    needle,
                    error
                );
            }

            // The other end of the same rule, and it is the one that is easy to
            // assume: a negative index is *not* out of bounds. SPEC.md §"List
            // Operations" makes it count from the end, so `-1` is the last item.
            // `.type` reports that value rather than refusing it, and pinning it
            // here is what stops a later "fix" turning it into an error.
            let last = repl
                .type_answer("[1, 2, 3][-1]")
                .expect("-1 names an element");
            assert!(
                last[0].contains("value: 3"),
                "index -1 counts from the end: {:?}",
                last
            );
            let first = repl
                .type_answer("[1, 2, 3][0]")
                .expect("index 0 is the first");
            assert!(
                first[0].contains("value: 1"),
                "and index 0 is the first: {:?}",
                first
            );

            assert!(
                repl.type_answer("counter").is_ok(),
                "a question the session can answer still is answered afterwards"
            );
            assert!(
                repl.which_answer("counter")
                    .contains(&"counter: session binding".to_string()),
                "and the session is not a fresh one: the binding is still here"
            );
        }

        /// A name nothing binds is an error from the session's own analyzer, not
        /// a panic and not a value of some kind called "unknown".
        #[test]
        fn edge_type_of_an_unknown_name_is_a_clean_error() {
            let mut repl = session_with_bindings();

            let error = repl
                .type_answer("nosuchname")
                .expect_err("a name nothing binds cannot be typed");

            assert!(
                error.contains("nosuchname"),
                "the error names what could not be found: {}",
                error
            );
            assert!(
                error.starts_with("Error:"),
                "and is reported as the error it is: {}",
                error
            );
        }

        /// `.type` with nothing after it is the parser's missing-argument report,
        /// not a command that evaluated the empty string and printed a value.
        #[test]
        fn edge_type_with_no_argument_is_refused_before_anything_runs() {
            let mut repl = Repl::new();

            assert_eq!(
                ReplCommand::parse(":type"),
                ReplCommand::MissingArgument {
                    command: "type",
                    usage: "<expression>"
                },
                "an expression is what :type is asked about"
            );
            assert!(
                repl.handle_command(":type"),
                "and it is a command, not a program to evaluate"
            );
            assert!(
                repl.variables.is_empty(),
                "nothing was bound by a command that had nothing to evaluate"
            );
        }

        /// `.which` names a builtin, names this session's own binding over it, and
        /// says "unknown" for a name nothing binds — three answers from one
        /// command, which is what makes the lookup order visible.
        #[test]
        fn which_distinguishes_a_builtin_a_session_binding_and_an_unknown_name() {
            let mut repl = Repl::new();

            let builtin = repl.which_answer("uppercase").join("\n");
            assert!(
                builtin.contains("builtin"),
                "a registered builtin is named as one: {}",
                builtin
            );

            feed(&mut repl, &["set uppercase to 5"]);
            let shadowed = repl.which_answer("uppercase").join("\n");
            assert!(
                shadowed.contains("session binding"),
                "and the session binding that shadows it is reported as what the next \
                 line reaches: {}",
                shadowed
            );
            assert!(
                shadowed.contains("<session>:1"),
                "with the line it was written on: {}",
                shadowed
            );

            let unknown = repl.which_answer("nosuchname").join("\n");
            assert!(
                unknown.contains("unknown"),
                "a name nothing binds is unknown, not a builtin: {}",
                unknown
            );
        }

        /// A dotted name is one of two things, and the difference matters: a
        /// stdlib member is in the registry whether or not this session has ever
        /// heard of it, while a module member exists only because the session
        /// imported something. Asking "is it bound?" cannot tell them apart —
        /// every builtin is bound in the VM's globals — so `files.read` came back
        /// as a member of a module the session never imported.
        #[test]
        fn which_tells_a_stdlib_member_from_a_modules_own_member() {
            let mut repl = Repl::new();
            feed(&mut repl, &["import MathUtils"]);

            let stdlib_member = repl.which_answer("files.read").join("\n");
            assert!(
                stdlib_member.contains("module member")
                    && !stdlib_member.contains("session module member"),
                "a stdlib member is in the registry, not in this session: {}",
                stdlib_member
            );
            assert!(
                stdlib_member.contains("callable as files.read"),
                "and it is named the way a program writes it: {}",
                stdlib_member
            );

            let own = repl.which_answer("MathUtils.circle_area").join("\n");
            assert!(
                own.contains("session module member"),
                "a member of a module this session imported is the session's own: {}",
                own
            );
            assert!(
                own.contains("of module MathUtils"),
                "and names the module it belongs to: {}",
                own
            );

            // The module name itself is a binding, and the line that made it is
            // the `import`.
            let module = repl.which_answer("MathUtils").join("\n");
            assert!(
                module.contains("session binding") && module.contains("import MathUtils"),
                "the module is bound by the import that reached it: {}",
                module
            );
        }

        /// A re-`set` of a name already bound reports the line the *new* value was
        /// written on. A before-and-after comparison of bindings could not see
        /// this, which is why `.which` reads the declaration rather than the
        /// change.
        #[test]
        fn edge_which_reports_the_latest_set_of_a_name() {
            let mut repl = Repl::new();
            feed(&mut repl, &["set x to 1", "set y to 2", "set x to 99"]);

            let answer = repl.which_answer("x").join("\n");

            assert!(
                answer.contains("<session>:3"),
                "the third line is where x was last written: {}",
                answer
            );
            assert!(
                answer.contains("set x to 99"),
                "and the line is quoted so it can be checked: {}",
                answer
            );
        }

        /// `.doc` answers from the registry, so the expectation is derived from it
        /// rather than written out: a member added to the language is listed
        /// without this test being edited.
        #[test]
        fn doc_lists_the_members_the_registry_registers() {
            let repl = Repl::new();

            for module in stdlib::MODULES {
                let answer = repl
                    .module_doc_answer(module)
                    .unwrap_or_else(|error| panic!("{} is a module: {}", module, error));
                for (listed, members) in stdlib::MODULE_MEMBERS.iter().filter(|(m, _)| m == module)
                {
                    for member in *members {
                        let spelling = format!("{}.{}", listed, member);
                        assert!(
                            answer.iter().any(|line| line.contains(&spelling)),
                            "{:?} lists {}",
                            module,
                            spelling
                        );
                    }
                }
            }
        }

        /// A name that is not a module is refused by name, and the refusal names
        /// the modules that do exist — an empty listing reads like a module with
        /// nothing in it.
        #[test]
        fn edge_doc_of_an_unknown_module_is_refused_by_name() {
            let repl = Repl::new();

            let error = repl
                .module_doc_answer("nosuchmodule")
                .expect_err("a name that is not a module gets no listing");

            assert!(
                error.contains("nosuchmodule"),
                "the refusal names the module asked for: {}",
                error
            );
            assert!(
                error.contains("json"),
                "and the ones that do exist, so the user can see the shape of the answer: {}",
                error
            );

            // The empty name is the boundary of the same refusal: it is not the
            // module that happens to have no members.
            assert!(repl.module_doc_answer("").is_err());
            assert!(
                repl.module_doc_answer("JSON").is_err(),
                "a module name is spelled exactly as it is registered"
            );
        }

        /// `.sighelp` reads the shape table, and every builtin on it answers.
        #[test]
        fn sighelp_answers_for_every_registered_builtin() {
            let repl = Repl::new();

            for name in stdlib::BUILTIN_NAMES {
                let answer = repl
                    .sighelp_answer(name)
                    .unwrap_or_else(|error| panic!("{} has a shape: {}", name, error));
                assert!(
                    answer[0].contains(name),
                    "{} answers under its own name: {:?}",
                    name,
                    answer
                );
            }
        }

        /// The boundary of a shape is the empty name, and a session function's
        /// shape is read from the declaration rather than guessed.
        #[test]
        fn edge_sighelp_of_an_empty_name_and_of_a_session_function() {
            let repl = session_with_bindings();

            assert!(
                repl.sighelp_answer("").is_err(),
                "an empty name is not a builtin with no parameters"
            );

            let answer = repl
                .sighelp_answer("double")
                .expect("a function the session declared is a call it can make")
                .join("\n");
            assert!(
                answer.contains("parameters: n"),
                "the declared parameters are the shape: {}",
                answer
            );
            assert!(
                answer.contains("arity:       1"),
                "and how many of them there are: {}",
                answer
            );
        }

        /// Every new command is in `:help`. The help text is built from the table,
        /// so this cannot drift unless the table does — but it is asserted by name
        /// because a command that works and is not documented is the defect the
        /// finding describes for the whole table.
        #[test]
        fn edge_help_names_every_command_that_inspects_code() {
            let help = ReplCommand::help_lines().join("\n");

            for command in ["type", "doc", "which", "sighelp"] {
                assert!(
                    help.contains(&format!(":{}", command)),
                    ":help names :{}: {}",
                    command,
                    help
                );
            }
        }

        /// And the completer offers them, which is the other half of "the command
        /// exists": a command nobody can complete is a command nobody types.
        #[test]
        fn edge_the_completer_offers_the_introspection_commands() {
            let repl = Repl::new();

            // Through a sigil, because that is how a command word is reached:
            // a bare `ty` offers the language's names, not `:type`. The sigil is
            // put back on what it offers, so that is what the answer is checked
            // against.
            for command in ["type", "doc", "which", "sighelp"] {
                let completions = repl.complete(&format!(":{}", command));
                let expected = format!(":{}", command);
                assert!(
                    completions.contains(&expected),
                    "{} is completed: {:?}",
                    expected,
                    completions
                );
            }
        }
    }

    /// The session surface: the greeting, the prompt, and `!!`.
    ///
    /// These drive [`Repl::session_loop`] over a [`ScriptedSource`] that records the
    /// prompt it is asked at, which is the only place a prompt is observable — the
    /// reader is what a user sees.
    mod surface {
        use super::super::{MAX_PROMPT_BYTES, REPEAT};
        use super::{Repl, ScriptedSource, SessionEnd};
        use crate::repl::banner;
        use crate::repl::editor::Line;
        use crate::repl::style::Style;
        use std::cell::RefCell;
        use std::rc::Rc;

        /// Runs `lines` through the session and returns every prompt it asked at,
        /// including the one it asked at before the session ended.
        ///
        /// The lines go in as a pipe delivers them — with the newline the reader
        /// stopped at still attached — because that is the shape every
        /// integration test drives the real binary with, and it is the only shape
        /// that can tell a session which strips a line terminator from one that
        /// has never been asked to. A scripted source that fed bare lines passed
        /// while a piped `:prompt` grew a newline into every prompt after it.
        fn prompts_for(repl: &mut Repl, lines: &[&str]) -> Vec<String> {
            let mut source = ScriptedSource::new(
                lines
                    .iter()
                    .map(|line| Ok(Line::Text(format!("{line}\n"))))
                    .collect(),
            );
            let names = Rc::new(RefCell::new(repl.session_names()));
            repl.session_loop(&mut source, &names);
            source.prompts().to_vec()
        }

        /// The greeting a session prints under the wordmark, with colour forced off.
        fn plain_greeting() -> String {
            banner::print_greeting(&mut Vec::new(), &Style::forced(false))
                .ok()
                .map(|_| ())
                .unwrap_or_default();
            banner::greeting_text(&Style::forced(false))
        }

        #[test]
        fn the_greeting_names_the_crate_version_the_engine_and_what_the_keys_do() {
            let greeting = plain_greeting();

            assert!(
                greeting.contains(&format!("Redblue v{}", env!("CARGO_PKG_VERSION"))),
                "the greeting names the version the crate is, got:\n{greeting}"
            );
            assert!(
                !greeting.contains("v0.1.0"),
                "and the stale version it used to say is gone, got:\n{greeting}"
            );
            assert!(
                greeting.contains(banner::ENGINE),
                "the greeting names the engine in use, got:\n{greeting}"
            );
            assert!(
                greeting.contains("TAB") && greeting.contains("Up arrow"),
                "a first-time user is told what the keys do without reading the docs, \
             got:\n{greeting}"
            );
        }

        /// The greeting with colour off has to be the text and nothing else: an
        /// escape sequence in a banner is visible to every user as garbage, and a
        /// blank line where a sentence should be is the same kind of failure.
        #[test]
        fn edge_the_greeting_with_colour_off_is_plain_aligned_and_complete() {
            let plain = plain_greeting();
            let coloured = banner::greeting_text(&Style::forced(true));

            assert!(
                !plain.contains('\u{1b}'),
                "no escape sequence survives with colour off, got:\n{plain:?}"
            );
            // Three lines and a blank one: the greeting is a block, and the blank
            // line is what separates it from the first prompt rather than a fourth
            // sentence of it.
            let lines: Vec<&str> = plain.lines().collect();
            assert_eq!(
                lines.len(),
                4,
                "the greeting is three lines and the blank that follows them, got {lines:?}"
            );
            assert_eq!(
                lines[3], "",
                "and the fourth line is the blank separator, got {lines:?}"
            );
            for line in &lines[..3] {
                assert!(!line.trim().is_empty(), "no blank line in {lines:?}");
                assert!(
                    !line.starts_with(' '),
                    "no line is indented, so the block reads as a block: {lines:?}"
                );
            }
            // Every escape the coloured form adds is a whole paint: it opens and it
            // resets. A half-open one leaves the rest of the terminal coloured.
            assert_eq!(
                coloured.matches("\u{1b}[0m").count(),
                coloured.matches('\u{1b}').count() / 2,
                "every escape in the coloured greeting is closed, got:\n{coloured:?}"
            );
            assert!(
                plain.len() < coloured.len(),
                "colour is the difference between the two renderings, and only that"
            );
        }

        /// A block is read one line at a time, and each continuation says which line
        /// of the block it is.
        ///
        /// The number is the line about to be typed, so a three-line block asks for
        /// lines 2 and 3 and never for a 4th — and the prompt after the closing `end`
        /// is the primary one again, which is what says the block closed.
        #[test]
        fn a_block_is_read_at_a_prompt_numbered_by_the_line_of_the_block() {
            let mut repl = Repl::new();
            let prompts = prompts_for(
                &mut repl,
                &["repeat 1 times", "    say \"body\"", "end", ":quit"],
            );

            assert_eq!(
                prompts,
                vec![">>> ", "..  2 ", "..  3 ", ">>> "],
                "each line of the block is asked for by its own number"
            );
        }

        /// The prompt is the user's, not merely coloured: `:prompt` replaces the
        /// text the next line is read at, and every placeholder it documents is
        /// filled in.
        #[test]
        fn a_prompt_format_becomes_the_prompt_the_next_line_is_read_at() {
            let mut repl = Repl::new();
            repl.history.push("set x to 1".to_string());

            assert!(
                repl.handle_command(":prompt rb{version}/{history}/{engine} "),
                ":prompt is a command"
            );
            assert_eq!(
                prompts_for(&mut repl, &[""])[0],
                format!("rb{}/{}/{} ", env!("CARGO_PKG_VERSION"), 1, banner::ENGINE),
                "the format is what the next prompt is, with its placeholders filled"
            );

            assert!(
                repl.handle_command(":prompt"),
                ":prompt with no argument is the command too, not a missing argument"
            );
            assert_eq!(
                prompts_for(&mut repl, &[""])[0],
                ">>> ",
                "and with no argument it puts the default back"
            );
        }

        /// A format string naming a placeholder that does not exist is refused by
        /// name, and the prompt the session had is kept: a prompt is printed before
        /// every line, so one full of `{nope}` would be read a thousand times a day.
        #[test]
        fn edge_an_unknown_placeholder_is_refused_by_name_and_the_prompt_is_kept() {
            let mut repl = Repl::new();

            let refused = repl.set_prompt_format(Some("rb{nope} "));
            assert!(
                refused.is_err(),
                "a placeholder the prompt does not know is refused, got {refused:?}"
            );
            let message = refused.unwrap_err();
            assert!(
                message.contains("{nope}"),
                "the refusal names the placeholder that was refused, got: {message}"
            );
            assert_eq!(
                prompts_for(&mut repl, &[""])[0],
                ">>> ",
                "and the prompt the session had is still the prompt"
            );

            let unclosed = repl.set_prompt_format(Some("rb{version"));
            assert!(
                unclosed.is_err(),
                "a brace that is never closed is refused too, got {unclosed:?}"
            );
            assert!(
                unclosed.unwrap_err().contains('{'),
                "and the report shows the brace it could not read past"
            );

            // The other end of the same rule: a `}` that closes nothing. The
            // placeholders are a closed set, so a brace is either one of them or
            // a mistake — and `:prompt foo}` used to be accepted and rendered
            // literally, contradicting the claim the refusal above is made for.
            let stray = repl.set_prompt_format(Some("rb>} "));
            assert!(
                stray.is_err(),
                "a brace that closes nothing is refused the same way, got {stray:?}"
            );
            assert!(
                stray.unwrap_err().contains('}'),
                "and the report shows the brace it could not match"
            );
            assert!(
                repl.set_prompt_format(Some("rb{version} ok}")).is_err(),
                "including one behind a placeholder that did open, rather than only \
                 at the start of a format"
            );
            assert!(
                repl.set_prompt_format(Some("rb} {version} ")).is_err(),
                "and one *before* a placeholder, which is the same mistake in the \
                 other order"
            );

            assert!(
                repl.set_prompt_format(None).is_ok(),
                "and `:prompt` with no argument still restores the default"
            );
        }

        /// A format string is held for the rest of the session and rendered before
        /// every line the session reads, so there has to be a size at which it is
        /// refused — the same reason the block buffer has one.
        ///
        /// Both sides of the boundary are asserted, because a limit that refuses
        /// too little is not a limit and one that refuses too much is a bug: the
        /// default prompt is four bytes and `>>> ` is the smallest thing a session
        /// ever prints, so nothing anyone would type is near a kilobyte.
        #[test]
        fn edge_an_oversize_prompt_format_is_refused_and_the_prompt_is_kept() {
            let mut repl = Repl::new();
            let at_limit = "x".repeat(MAX_PROMPT_BYTES);
            let set = repl.set_prompt_format(Some(&at_limit));
            assert!(
                set.is_ok(),
                "a format of exactly {} bytes is allowed, got {:?}",
                MAX_PROMPT_BYTES,
                set.err()
            );

            let over = "x".repeat(MAX_PROMPT_BYTES + 1);
            let refused = repl.set_prompt_format(Some(&over));
            assert!(
                refused.is_err(),
                "one byte more is not, and a prompt is rendered once per line for \
                 the rest of the session"
            );
            let message = refused.unwrap_err();
            assert!(
                message.contains(&MAX_PROMPT_BYTES.to_string()),
                "the refusal names the limit, got: {message}"
            );
            assert!(
                message.len() < 200,
                "and quotes the size rather than the format, so refusing an \
                 oversize prompt is not itself a flood, got: {message}"
            );

            assert_eq!(
                prompts_for(&mut repl, &[""])[0],
                at_limit,
                "the prompt the session had is still the prompt"
            );
        }

        /// `!!` hands the history entry back to the parser as a command, so the
        /// entry has to be the line that was typed — including the whitespace at
        /// the end of it, for the one command whose argument is display text.
        ///
        /// A prompt ends in a space; that space is the gap between the prompt and
        /// what gets typed. Recorded trimmed, `:prompt rb> ` came back from `!!` as
        /// `:prompt rb>` and the prompt silently lost its gap, which is exactly
        /// the kind of change a repeat must not make. Every other line is still
        /// recorded trimmed, so a stray space at the end of a program does not
        /// reach the history file.
        #[test]
        fn edge_a_repeated_prompt_keeps_the_space_that_is_part_of_it() {
            let mut repl = Repl::new();
            let prompts = prompts_for(&mut repl, &[":prompt rb> ", "!!", ":quit"]);

            assert_eq!(
                prompts,
                vec![">>> ", "rb> ", "rb> "],
                "the repeat put the prompt back as it was typed, space and all"
            );
            assert_eq!(
                repl.history.get(0).map(String::as_str),
                Some(":prompt rb> "),
                "and the entry it repeated is the line as typed, not the same \
                 line trimmed — which is what a second `!!` would hand back"
            );

            let mut repl = Repl::new();
            prompts_for(&mut repl, &["set x to 1   ", ":quit"]);
            assert_eq!(
                repl.history.get(0).map(String::as_str),
                Some("set x to 1"),
                "a line that is not a display-text command is still recorded \
                 trimmed, so the history file does not collect stray spaces"
            );
        }

        /// A line ends where the reader stopped; the newline it stopped at is not
        /// part of it.
        ///
        /// A pipe delivers `:prompt rb> \n`, and the space at the end of that is
        /// part of the prompt while the newline after it is not. Kept on, the
        /// stored format ends in a newline and every prompt printed from it
        /// breaks the line before the user has typed anything — and the history
        /// entry, which is the same string, is two lines in a file that is one
        /// entry per line. Piped here through `prompts_for`, which feeds the
        /// shape a pipe actually delivers.
        #[test]
        fn edge_a_piped_prompt_command_leaves_no_newline_in_the_prompt_or_the_history() {
            let mut repl = Repl::new();
            let prompts = prompts_for(&mut repl, &[":prompt rb> ", "say \"piped\"", ":quit"]);

            assert_eq!(
                prompts,
                vec![">>> ", "rb> ", "rb> "],
                "the prompt is `rb> ` and nothing else, so what is typed lands \
                 after the space the user gave it"
            );
            for prompt in &prompts {
                assert!(
                    !prompt.contains(['\n', '\r']),
                    "no prompt carries the newline the line arrived with, got {prompt:?}"
                );
            }
            assert_eq!(
                repl.history.get(0).map(String::as_str),
                Some(":prompt rb> "),
                "and the entry is one line, so writing the history out is one line \
                 per entry rather than a blank line for every prompt that was set"
            );
            assert!(
                repl.history
                    .get(0)
                    .is_some_and(|entry| !entry.ends_with(['\n', '\r'])),
                "…and it is a line without a terminator of its own"
            );
        }

        /// `!!` is the last line typed, run again — including a line that failed,
        /// because a line that failed is very often the one being worked on.
        #[test]
        fn edge_repeat_reruns_a_line_that_failed_rather_than_skipping_it() {
            let mut repl = Repl::new();

            let refusal = repl.repeat();
            assert!(
                refusal.is_err(),
                "a session with nothing typed has nothing to repeat, got {refusal:?}"
            );
            assert!(
                refusal.unwrap_err().contains(REPEAT),
                "and the refusal names the line that asked for it"
            );

            // A session that stops right after the repeat: its history is the failed
            // line and the `!!`, so what `!!` reached is readable afterwards.
            let mut stopped = Repl::new();
            let mut source = ScriptedSource::new(vec![
                Ok(Line::Text("1 / 0".to_string())),
                Ok(Line::Text("!!".to_string())),
            ]);
            let names = Rc::new(RefCell::new(stopped.session_names()));
            stopped.session_loop(&mut source, &names);
            assert_eq!(
                stopped.repeat_target(),
                Some("1 / 0"),
                "the failed line is what `!!` reaches, not the `!!` itself and not \
             nothing"
            );
            assert_eq!(
                stopped.history.search("1 / 0"),
                Some("1 / 0".to_string()),
                "and the repeat did not push a second copy of the line it repeated"
            );

            // And the session carries on afterwards: a repeat that swallowed the
            // rest of the session would leave the lines after it unrun.
            let mut source = ScriptedSource::new(vec![
                Ok(Line::Text("1 / 0".to_string())),
                Ok(Line::Text("!!".to_string())),
                Ok(Line::Text("set after_repeat to 40 + 1".to_string())),
                Ok(Line::Text(":quit".to_string())),
            ]);
            let names = Rc::new(RefCell::new(repl.session_names()));
            let end = repl.session_loop(&mut source, &names);

            assert_eq!(end, SessionEnd::Quit, "the session survived the repeat");
            assert!(
                repl.complete("after_repeat")
                    .contains(&"after_repeat".to_string()),
                "the line typed after the repeat is the one that ran next, so the \
             repeat did not swallow the session"
            );
        }

        /// `!!` twice reaches the same line both times: the repeats in between are
        /// not what it finds, or the second one would repeat the first.
        #[test]
        fn edge_a_second_repeat_reaches_the_same_line_as_the_first() {
            let mut repl = Repl::new();
            repl.history.push("set x to 1".to_string());
            repl.history.push("!!".to_string());
            repl.history.push("!!".to_string());

            assert_eq!(
                repl.repeat_target(),
                Some("set x to 1"),
                "two `!!` in the history are not a line to repeat"
            );
        }

        /// `:reset` clears the bindings and leaves the history — a session that has
        /// been reset is a session at a prompt, and the record of what was typed is
        /// not part of what a reset forgets.
        #[test]
        fn edge_repeat_still_works_after_a_reset() {
            let mut repl = Repl::new();
            repl.history.push("set x to 1".to_string());
            repl.reset_session();

            assert_eq!(
                repl.repeat_target(),
                Some("set x to 1"),
                "the history outlives a reset, so `!!` still has something to repeat"
            );
        }
    }

    /// `.time` and `.bench` — the session measuring itself.
    ///
    /// Before this there was no way to ask inside the REPL. The numbers are wall
    /// time and are nondeterministic by nature, so every assertion here is about
    /// the *shape* of the report: which figures it carries, that they are finite
    /// and positive, that the mean is the total over the runs, and that the
    /// samples are ordered. No test asserts how long anything took.
    mod measure {
        use super::{feed, Repl};
        use crate::repl::commands::ReplCommand;
        use crate::value::Value;

        /// The number and unit a labelled line carries, read back out of the text.
        ///
        /// Parsed rather than matched, because a report a test can only check for
        /// the presence of a word in is a report whose numbers could be anything.
        fn reading(lines: &[String], label: &str) -> (f64, String) {
            let line = lines
                .iter()
                .find(|line| line.trim_start().starts_with(label))
                .unwrap_or_else(|| panic!("{} should be reported: {:?}", label, lines));
            let rest = line.trim_start()[label.len()..].trim();
            let (number, unit) = rest
                .split_once(char::is_whitespace)
                .unwrap_or_else(|| panic!("{:?} should carry a number and a unit", rest));
            (
                number
                    .parse()
                    .unwrap_or_else(|_| panic!("{:?} should be a number", number)),
                unit.trim().to_string(),
            )
        }

        fn assert_finite_positive(value: f64, what: &str) {
            assert!(value.is_finite(), "{} must be finite: {}", what, value);
            assert!(value > 0.0, "{} must be positive: {}", what, value);
        }

        /// `.time 1 + 1` answers with a wall-time total and a per-run mean, in one
        /// unit, over the ten runs it makes when it is not told otherwise.
        #[test]
        fn time_reports_a_wall_time_total_and_a_mean_over_ten_runs() {
            let mut repl = Repl::new();

            let lines = repl
                .time_answer("1 + 1")
                .expect("1 + 1 is something :time can run");
            let joined = lines.join("\n");

            assert!(
                joined.contains("wall time"),
                "the numbers are wall time and say so: {:?}",
                lines
            );
            assert!(
                joined.contains("10 run"),
                "and how many runs the figure covers: {:?}",
                lines
            );

            let (total, total_unit) = reading(&lines, "total:");
            let (mean, mean_unit) = reading(&lines, "mean:");
            assert_eq!(
                total_unit, mean_unit,
                "one unit for the whole report, so the two figures can be compared: {:?}",
                lines
            );
            assert!(
                matches!(total_unit.as_str(), "ns" | "µs" | "ms" | "s"),
                "a labelled unit: {:?}",
                lines
            );
            assert_finite_positive(total, "the total");
            assert_finite_positive(mean, "the mean");
            assert!(
                total >= mean,
                "ten runs take no less than one of them: {} against {}",
                total,
                mean
            );
            // The mean is the total over the runs, to within the three decimals
            // both are printed to.
            assert!(
                (total - mean * 10.0).abs() <= 0.02,
                "the mean is the total over the runs: total {} against mean {} x 10",
                total,
                mean
            );
        }

        /// `.bench` answers with the distribution over its hundred runs, and the
        /// distribution is ordered.
        #[test]
        fn bench_reports_min_median_max_and_mean_over_a_hundred_runs() {
            let mut repl = Repl::new();

            let lines = repl
                .bench_answer("say 1")
                .expect("`say 1` is something :bench can run");
            let joined = lines.join("\n");
            assert!(
                joined.contains("wall time"),
                "the numbers are wall time and say so: {:?}",
                lines
            );
            assert!(
                joined.contains("100 run"),
                "a hundred runs is what :bench measures: {:?}",
                lines
            );

            let (min, min_unit) = reading(&lines, "min:");
            let (median, median_unit) = reading(&lines, "median:");
            let (max, max_unit) = reading(&lines, "max:");
            let (mean, mean_unit) = reading(&lines, "mean:");

            assert_eq!(min_unit, median_unit, "one unit for the whole report");
            assert_eq!(min_unit, max_unit, "one unit for the whole report");
            assert_eq!(min_unit, mean_unit, "one unit for the whole report");

            for (value, what) in [
                (min, "min"),
                (median, "median"),
                (max, "max"),
                (mean, "mean"),
            ] {
                assert_finite_positive(value, what);
            }
            assert!(
                min <= median && median <= max,
                "the samples are ordered: min {} median {} max {}",
                min,
                median,
                max
            );
            assert!(
                mean >= min && mean <= max,
                "a mean is one of the samples' middle: mean {} against {}..{}",
                mean,
                min,
                max
            );
        }

        /// The timed code runs *in the session*: it reads the bindings the session
        /// already holds, and what it binds is still there afterwards. A throwaway
        /// interpreter could answer `.time x + 1` for a session that never held
        /// `x`, and would throw the counts away.
        #[test]
        fn timed_code_runs_in_the_session_it_was_typed_in() {
            let mut repl = Repl::new();
            feed(&mut repl, &["set x to 3"]);

            let lines = repl
                .time_answer("x + 1 4 times")
                .expect("x is bound, so the timed expression resolves");
            assert!(
                lines.join("\n").contains("4 run"),
                "the run count the user gave is the run count made: {:?}",
                lines
            );

            feed(&mut repl, &["set hits to 0"]);
            repl.time_answer("set hits to hits + 1 5 times")
                .expect("a statement is something :time can run");

            assert_eq!(
                repl.lookup("hits").cloned(),
                Some(Value::Number(5.0)),
                "five runs of an incrementing statement leave five, so the runs \
                 happened in this session's own VM"
            );
        }

        /// A failing expression is reported once, and the loop stops: the run
        /// counter proves the failure was not repeated.
        #[test]
        fn edge_a_failing_expression_reports_its_error_once_and_runs_once() {
            let mut repl = Repl::new();
            feed(
                &mut repl,
                &[
                    "set hits to 0",
                    "to bump()\n    set hits to hits + 1\n    give back 1 / 0\nend",
                ],
            );

            let error = repl
                .time_answer("bump() 10 times")
                .expect_err("dividing by zero is not something :time can time");

            assert!(
                error.contains("Error"),
                "the underlying error, not a silent success: {}",
                error
            );
            assert_eq!(
                repl.lookup("hits").cloned(),
                Some(Value::Number(1.0)),
                "the failing run happened once and the loop stopped, rather than \
                 timing the same failure ten times"
            );

            // And the command reaches the session rather than being handed to the
            // parser as source: `.time bump()` is not Redblue, so a `.time` the
            // table never heard of would have run nothing at all and left the
            // counter where it was.
            let mut dispatched = Repl::new();
            feed(
                &mut dispatched,
                &[
                    "set hits to 0",
                    "to bump()\n    set hits to hits + 1\n    give back 1 / 0\nend",
                ],
            );
            assert!(
                dispatched.handle_command(".time bump() 10 times"),
                "`.time` is dispatched as a command"
            );
            assert_eq!(
                dispatched.lookup("hits").cloned(),
                Some(Value::Number(1.0)),
                "the dispatched command ran the failing expression once, as well"
            );
        }

        /// Both commands insist on an expression, and the report says which one
        /// was missing rather than timing nothing. Both sigils reach them.
        #[test]
        fn edge_time_with_no_argument_is_refused_before_anything_runs() {
            for (input, command, usage) in [
                (":time", "time", "<expression> [n times]"),
                (".time", "time", "<expression> [n times]"),
                (":bench", "bench", "<expression> [n times]"),
                (".bench  ", "bench", "<expression> [n times]"),
            ] {
                assert_eq!(
                    ReplCommand::parse(input),
                    ReplCommand::MissingArgument { command, usage },
                    "{:?} is a command that was given no expression",
                    input
                );
            }

            // And given one, each carries it whole — the run count included.
            assert_eq!(
                ReplCommand::argument_text(&ReplCommand::parse(".time x + 1 4 times")),
                Some("x + 1 4 times"),
                "the whole argument is the measurement, run count and all"
            );
            assert_eq!(
                ReplCommand::argument_text(&ReplCommand::parse(".bench say 1")),
                Some("say 1"),
                "a statement is a thing to measure, not just an expression"
            );
        }

        /// A run count that is not a number is refused by name, and a count of
        /// zero or fewer is refused rather than divided by.
        #[test]
        fn edge_a_run_count_that_is_not_a_positive_number_is_refused_by_name() {
            let mut repl = Repl::new();

            let not_a_number = repl
                .bench_answer("1 + 1 many times")
                .expect_err("'many' is not a count");
            assert!(
                not_a_number.contains("bench") && not_a_number.contains("many"),
                "the refusal names the command and the count it could not read: {}",
                not_a_number
            );

            for count in ["0", "-4"] {
                let refused = repl
                    .time_answer(&format!("1 + 1 {} times", count))
                    .expect_err("no runs is no measurement");
                assert!(
                    refused.contains(count.trim_start_matches('-')) && refused.contains("time"),
                    "{} runs is refused by name rather than divided by: {}",
                    count,
                    refused
                );
            }

            let beyond = repl
                .bench_answer("1 + 1 100000000 times")
                .expect_err("a count past the cap is a hang, not a measurement");
            assert!(
                beyond.contains("bench"),
                "the cap is reported against the command that was asked for: {}",
                beyond
            );
        }

        /// An expression that cannot be run at all is the pipeline's own error,
        /// reported rather than panicked on — and a unicode expression is measured
        /// like any other.
        #[test]
        fn edge_a_broken_expression_is_the_language_error_and_unicode_measures_fine() {
            let mut repl = Repl::new();

            let broken = repl
                .time_answer("1 +")
                .expect_err("an unfinished expression does not run");
            assert!(
                broken.contains("Error"),
                "the parser's own error: {}",
                broken
            );
            assert!(
                repl.time_answer("\"héllo \" + \"🙂\"").is_ok(),
                "a unicode expression is timed like any other"
            );
        }
    }

    /// A failure a session reports points at the line the user typed.
    ///
    /// Every failure already carries a position and [`Error::render`] already
    /// draws it — label, location, the offending source line and a caret under
    /// the column. A session used to print only the first two of those through
    /// [`std::fmt::Display`], so the same program showed more at `rb run` than
    /// at the prompt, and the one place a reader can try a line and see what it
    /// does was the one place that refused to show them which part of it was
    /// wrong.
    ///
    /// The properties pinned here are the ones a REPL reader depends on: the
    /// line and caret are there, they are the language's own rendering rather
    /// than a second format, they are positioned by the buffer and not by the
    /// history, and a failure with no position renders as the message alone
    /// rather than reaching for a line that is not there.
    mod error_rendering {
        use super::Repl;
        use crate::error::{Error, Span};
        use crate::repl::introspect::SESSION_FILE;
        use crate::repl::style::{Paint, Style};
        use crate::repl::{is_caret_line, render_error};

        /// Strips every escape sequence, so a test asserts on the text and
        /// cannot fail because the palette changed.
        fn plain(rendered: &str) -> String {
            let mut out = String::new();
            let mut chars = rendered.chars();
            while let Some(c) = chars.next() {
                if c == '\u{1b}' {
                    for skip in chars.by_ref() {
                        if skip == 'm' {
                            break;
                        }
                    }
                } else {
                    out.push(c);
                }
            }
            out
        }

        /// The `line:column` a rendered failure is drawn at, with the file
        /// dropped.
        ///
        /// The file is the one half that must differ: a session's failure is in
        /// `<session>` and the same program run from disk is in its own path.
        /// The last two fields are what both have to agree on.
        fn position(rendered: &str) -> String {
            let line = rendered
                .lines()
                .find(|line| line.trim_start().starts_with("-->"))
                .unwrap_or_else(|| panic!("no location in:\n{}", rendered));
            let location = line.trim().trim_start_matches("-->").trim();
            let fields: Vec<&str> = location.split(':').collect();
            assert!(fields.len() >= 2, "no line:column in {}", line);
            fields[fields.len() - 2..].join(":")
        }

        /// The `N | text` line drawn for buffer line `n`.
        fn source_line(rendered: &str, n: usize) -> &str {
            rendered
                .lines()
                .find(|line| line.starts_with(&format!("{} | ", n)))
                .unwrap_or_else(|| {
                    panic!("line {} of the buffer is not drawn in:\n{}", n, rendered)
                })
        }

        /// The caret or underline line: the drawn line the painter calls a caret
        /// line. It asks [`is_caret_line`] rather than repeating it, so a test
        /// cannot pass on a reading of the drawing that production does not have.
        fn caret(rendered: &str) -> &str {
            rendered
                .lines()
                .find(|line| is_caret_line(line))
                .unwrap_or_else(|| panic!("no caret in:\n{}", rendered))
        }

        /// Runs `code` in a session and hands back the failure it was given,
        /// rather than the text it printed.
        fn failure_of(code: &str) -> Error {
            let mut repl = Repl::new();
            repl.run_code(code)
                .expect_err("the program under test is meant to fail")
        }

        /// What a session prints about `code`, with colour off and the escapes
        /// stripped, so what is asserted on is the text a reader sees.
        fn session_render(code: &str) -> String {
            plain(&render_error(
                &Style::forced(false),
                code,
                SESSION_FILE,
                &failure_of(code),
            ))
        }

        /// The property the finding is about: a failure is drawn with the line it
        /// happened on, and the caret is put where the failure says.
        ///
        /// `say nosuchvar` fails at column 5, under the name: the analyzer
        /// records where a name was written, so a diagnostic points at the thing
        /// that is wrong rather than at the statement that read it. `set x to 1 +`
        /// is the other shape — a parser failure that is not about a name — and
        /// it is here because a caret pinned to the first column would pass the
        /// first and fail the second.
        #[test]
        fn a_failure_a_line_produces_is_drawn_with_that_line_under_it() {
            let code = "say nosuchvar";
            let rendered = session_render(code);

            assert!(
                rendered.contains("--> <session>:1:5"),
                "the failure names the file, the line and the column it failed at:\n{}",
                rendered
            );
            assert_eq!(
                source_line(&rendered, 1),
                "1 | say nosuchvar",
                "the line the user typed is quoted back at them:\n{}",
                rendered
            );
            assert_eq!(
                caret(&rendered),
                "  |     ^",
                "the caret sits under the name, not under the `say` that read \
                 it:\n{}",
                rendered
            );
            assert!(
                rendered.starts_with("AnalyzerError: Unknown variable 'nosuchvar'"),
                "the message is unchanged — colour may wrap it, never rewrite it:\n{}",
                rendered
            );

            let unfinished = session_render("set x to 1 +");
            assert_eq!(
                position(&unfinished),
                "1:13",
                "a column past the first character is reported as itself:\n{}",
                unfinished
            );
            assert_eq!(
                caret(&unfinished),
                "  |             ^",
                "and the caret is drawn under that column, not under the start \
                 of the line:\n{}",
                unfinished
            );

            // A failure of another kind, drawn the same way: the label is the
            // failure's own, and the line is quoted whichever kind it was.
            let mismatch = session_render("say 1 + \"a\"");
            assert!(
                mismatch.starts_with("RuntimeError: Cannot add non-numbers"),
                "a runtime failure is labelled as one:\n{}",
                mismatch
            );
            assert_eq!(
                source_line(&mismatch, 1),
                "1 | say 1 + \"a\"",
                "and its line is quoted like any other:\n{}",
                mismatch
            );
        }

        /// A span covering more than one line is drawn the way the language
        /// draws it: every line it covers, then the `~~~` under where it opens.
        #[test]
        fn a_failure_covering_several_lines_is_drawn_across_them() {
            let code = "say \"unterminated\nstill going\n";
            let rendered = session_render(code);

            assert_eq!(
                source_line(&rendered, 1),
                "1 | say \"unterminated",
                "the line the string opened on is drawn whole:\n{}",
                rendered
            );
            assert_eq!(
                source_line(&rendered, 2),
                "2 | still going",
                "and so is the line it ran on to:\n{}",
                rendered
            );
            assert_eq!(
                caret(&rendered),
                "  |     ^~~~~~~~~~~~~~",
                "the underline covers the string, from the column it opened at:\n{}",
                rendered
            );
        }

        /// A session's failure is the rendering the file runner prints, byte for
        /// byte once colour is off, and it says the same place.
        ///
        /// Both halves are run for real — the session through the engine a REPL
        /// runs, the file through the same path `rb run` takes — so they cannot
        /// agree by agreeing with themselves.
        #[test]
        fn a_session_failure_says_the_same_place_run_does() {
            let code = "set total to 1\nsay total + nosuchvar\n";
            let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("target/tmp/repl-error-render");
            std::fs::create_dir_all(&dir).expect("scratch directory should be creatable");
            let path = dir.join("diagnostic.rb");
            std::fs::write(&path, code).expect("the file should be writable");
            let path = path.to_str().expect("a path built here is valid UTF-8");

            let from_file = crate::run_file_with_diagnostic(path)
                .expect_err("the program under test is meant to fail");
            let walked = plain(&from_file);

            let failure = failure_of(code);
            let in_session = plain(&render_error(
                &Style::forced(false),
                code,
                SESSION_FILE,
                &failure,
            ));

            assert_eq!(
                position(&in_session),
                position(&walked),
                "a line and the same program in a file must fail at the same \
                 line and column\nsession:\n{}\nfile:\n{}",
                in_session,
                walked
            );
            assert_eq!(
                walked,
                render_error(&Style::forced(false), code, path, &failure),
                "the two must differ in the file they name and in nothing else:\n\
                 session:\n{}\nfile:\n{}",
                in_session,
                walked
            );
            assert_eq!(
                in_session.replace(SESSION_FILE, path).lines().count(),
                walked.lines().count(),
                "both draw the label, the location, the source line and the caret:\n\
                 session:\n{}\nfile:\n{}",
                in_session,
                walked
            );
            assert_eq!(
                caret(&walked),
                "  |             ^",
                "the source line and its caret are drawn, and the caret is under \
                 the name the failure names:\n{}",
                walked
            );
        }

        /// The other engine reports the same failure in the same place.
        ///
        /// A session runs one engine, so that the two agree is not something a
        /// REPL test can watch happen; it is pinned here by running the bytecode
        /// VM over the same text and diffing what the two render.
        #[test]
        fn the_bytecode_engine_reports_the_same_failure_in_the_same_place() {
            let code = "set xs to [1, 2]\nsay xs[9]\n";

            let walked = failure_of(code);
            let chunk = crate::compile_source(code).expect("the program compiles to bytecode");
            let compiled = crate::bytecode::vm::BytecodeVm::new()
                .run(&chunk)
                .expect_err("index 9 is out of bounds in both engines");

            let from_walker = plain(&render_error(
                &Style::forced(false),
                code,
                SESSION_FILE,
                &walked,
            ));
            let from_bytecode = plain(&render_error(
                &Style::forced(false),
                code,
                SESSION_FILE,
                &compiled,
            ));

            assert_eq!(
                from_walker, from_bytecode,
                "the same failure, drawn the same way by both engines — the same \
                 message, the same line, the same column, the same caret\n\
                 tree-walker:\n{}\nbytecode:\n{}",
                from_walker, from_bytecode
            );
            assert!(
                from_bytecode.contains("Index 9 is out of bounds"),
                "and it is the failure that was expected, not some other:\n{}",
                from_bytecode
            );
        }

        /// A block fails on the line inside it that failed, not on the first
        /// line of the buffer and not on the prompt's own numbering.
        #[test]
        fn edge_a_failure_inside_a_block_points_into_the_block() {
            let code = "repeat 2 times\n    say total\nend";
            let rendered = session_render(code);

            assert_eq!(
                source_line(&rendered, 2),
                "2 |     say total",
                "the offending line is drawn from the block, numbered as the \
                 buffer numbers it:\n{}",
                rendered
            );
            assert!(
                !rendered.contains("\n1 | repeat"),
                "line 1 of the block is not where the failure was:\n{}",
                rendered
            );
            assert_eq!(
                position(&rendered),
                "2:9",
                "the position is line 2 of the block, not line 2 of the session:\n{}",
                rendered
            );
            assert_eq!(
                caret(&rendered),
                "  |         ^",
                "the caret is under `total` on the line it was written on:\n{}",
                rendered
            );
        }

        /// A failure with no position — an `Io` failure never has a span — draws
        /// the message and nothing else, and does not reach past the end of the
        /// buffer for a line to quote.
        #[test]
        fn edge_a_failure_with_no_position_renders_the_message_alone() {
            let rendered = plain(&render_error(
                &Style::forced(false),
                "",
                SESSION_FILE,
                &Error::Io("No such file or directory".to_string()),
            ));

            assert_eq!(
                rendered, "IoError: No such file or directory",
                "a failure with no position is its message and its label:\n{}",
                rendered
            );
            assert!(
                !rendered.contains("-->") && !rendered.contains('|'),
                "nothing is invented for a failure that has no place:\n{}",
                rendered
            );
        }

        /// A position past the end of the buffer draws the location and stops,
        /// rather than indexing a line that is not there.
        #[test]
        fn edge_a_position_beyond_the_buffer_renders_the_location_and_stops() {
            let code = "set x to 1\n";
            let rendered = plain(&render_error(
                &Style::forced(false),
                code,
                SESSION_FILE,
                &Error::Runtime("something went wrong".to_string(), Span::new(99, 3)),
            ));

            assert_eq!(
                position(&rendered),
                "99:3",
                "the location is still the failure's own:\n{}",
                rendered
            );
            assert_eq!(
                rendered.lines().count(),
                2,
                "no source line is drawn for a line the buffer does not have:\n{}",
                rendered
            );
        }

        /// An empty line is a line: a failure whose span is on one is drawn as a
        /// source line with nothing in it rather than as nothing at all.
        #[test]
        fn edge_a_failure_on_an_empty_line_is_drawn_as_an_empty_line() {
            let code = "\nset x to 1\nsay nope\n";
            let rendered = plain(&render_error(
                &Style::forced(false),
                code,
                SESSION_FILE,
                &Error::Runtime("out of fuel".to_string(), Span::new(1, 1)),
            ));

            assert_eq!(
                source_line(&rendered, 1),
                "1 | ",
                "the empty line the failure is on is drawn as it is:\n{}",
                rendered
            );
            assert_eq!(
                caret(&rendered),
                "  | ^",
                "and the caret still lands on its column:\n{}",
                rendered
            );
        }

        /// A long line is drawn whole and the caret still lines up with the
        /// column it names, whatever follows it.
        #[test]
        fn edge_a_wide_line_is_drawn_whole_and_the_caret_stays_under_its_column() {
            let head = "set x to nosuchvar //";
            let code = format!("{}{}", head, " ".repeat(500 - head.len()));
            assert_eq!(code.chars().count(), 500, "the line is 500 characters wide");

            let rendered = session_render(&code);

            assert_eq!(
                source_line(&rendered, 1),
                format!("1 | {}", code),
                "a wide line is drawn whole, not cut down to the terminal:\n{}",
                rendered
            );
            assert_eq!(
                caret(&rendered),
                "  |          ^",
                "the caret is under the name on the wide line, not under its \
                 first character:\n{}",
                rendered
            );
        }

        /// Unicode before and after the failure: the line is drawn with every
        /// character it was written with, and the caret does not drift.
        #[test]
        fn edge_a_line_with_wide_characters_is_drawn_intact() {
            let code = "say nosuchvar // 🎉 ünïcode 日本語";
            let rendered = session_render(code);

            assert!(
                rendered.contains(code),
                "the source line keeps every character it was written with:\n{}",
                rendered
            );
            assert_eq!(
                caret(&rendered),
                "  |     ^",
                "and the caret is where the failure says it is:\n{}",
                rendered
            );
        }

        /// Colour wraps a diagnostic and does not become one: the label, the
        /// location and the caret are painted, the user's own source line is
        /// left plain, and with colour off the rendering is the language's own,
        /// byte for byte.
        #[test]
        fn colour_paints_the_diagnostic_and_leaves_the_source_line_plain() {
            let code = "say nosuchvar";
            let error = failure_of(code);
            let off = Style::forced(false);
            let on = Style::forced(true);

            let uncoloured = render_error(&off, code, SESSION_FILE, &error);
            let painted = render_error(&on, code, SESSION_FILE, &error);

            assert_eq!(
                uncoloured,
                error.render(code, Some(SESSION_FILE)),
                "with colour off a session's failure is byte for byte what the \
                 language renders — no reformatting of its own:\n{}",
                uncoloured
            );
            assert_eq!(
                plain(&painted),
                uncoloured,
                "colour is the only difference between the two renderings"
            );
            assert!(
                painted.contains(
                    &on.paint(Paint::Error, "AnalyzerError: Unknown variable 'nosuchvar'",)
                ),
                "the label and message are painted:\n{:?}",
                painted
            );
            assert!(
                painted.contains(&on.paint(Paint::Error, &format!("  --> {}:1:5", SESSION_FILE))),
                "the location is painted:\n{:?}",
                painted
            );
            assert!(
                painted.contains("\n1 | say nosuchvar\n"),
                "the source line is the reader's own text and is left plain:\n{:?}",
                painted
            );
            assert!(
                painted.contains(&on.paint(Paint::Error, "  |     ^")),
                "the caret is painted:\n{:?}",
                painted
            );
        }

        /// Colour marks the failure and nothing else — in particular not the
        /// note the renderer leaves when it cuts a long range short, which has
        /// the blank gutter of a caret and is not one.
        #[test]
        fn colour_leaves_the_renderers_own_truncation_notice_unpainted() {
            // Longer than the renderer draws, so the notice is really there:
            // an unterminated string runs to end of input.
            let mut code = String::from("say \"unterminated");
            for n in 1..14 {
                code.push_str(&format!("\nline {n} of a string that never closes"));
            }
            let error = failure_of(&code);
            let on = Style::forced(true);
            let painted = render_error(&on, &code, SESSION_FILE, &error);
            let notice = plain(&render_error(
                &Style::forced(false),
                &code,
                SESSION_FILE,
                &error,
            ))
            .lines()
            .find(|line| line.contains("more line(s) not drawn"))
            .expect("a range longer than the drawing limit says what it left out")
            .to_string();

            assert!(
                !is_caret_line(&notice),
                "the notice is the renderer's own report, not a line of the \
                 drawing: {}",
                notice
            );
            assert!(
                painted.contains(&format!("\n{notice}\n")),
                "and it is left as the reader's plain text rather than painted as \
                 a failure:\n{:?}",
                painted
            );
            assert!(
                !painted.contains(&on.paint(Paint::Error, &notice)),
                "painting the notice would colour the reader's own report that \
                 the diagnostic was cut:\n{:?}",
                painted
            );
            assert!(
                painted.contains(&on.paint(Paint::Error, "   |     ^~~~~~~~~~~~~~")),
                "the underline of the range it does draw is still painted:\n{:?}",
                painted
            );
        }
    }
}
