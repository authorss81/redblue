pub mod banner;
pub mod bytecode_view;
pub mod commands;
pub mod completer;
pub mod debug_repl;
pub mod editor;
pub mod highlight;
pub mod history;
pub mod introspect;
pub mod measure;
pub mod render;
pub mod style;
pub mod transcript;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

pub use commands::ReplCommand;
pub use completer::ReplCompleter;
pub use editor::{Line, LineSource, SessionNames};
pub use history::ReplHistory;
pub use introspect::Definition;
pub use transcript::Transcript;

use crate::analyzer;
use crate::debugger::{lock, DebugAction, DebugSession, SharedDebugSession, StepMode};
use crate::error::Error;
use crate::interpreter::Vm;
use crate::lexer::{Lexer, TokenKind};
use crate::parser::{open_block_depth, Expr, Parser, Program, Stmt};
use crate::repl::style::{strip_escapes, Paint, Style};
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

/// How many names a `.edit` will try for its own directory under the temporary
/// one, before it reports that it could not have one.
///
/// One is the number that would be right if nobody else were sharing the
/// directory. This is the number that is right if somebody is: a name that is
/// taken is somebody else's or an earlier edit's, and the next one is not.
const SCRATCH_DIR_ATTEMPTS: usize = 64;

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

/// One step of the guided tour `:tutorial` prints.
///
/// A step is a thing to try, the code to try it with, and the hint for the
/// reader who does not know what to type next. The hint is what `.hint` prints,
/// which is the difference between a tour and a wall of examples: a wall can
/// only be read, a step can be taken.
struct TutorialStep {
    title: &'static str,
    body: &'static str,
    /// A whole program, not a fragment: the test at the bottom of this module
    /// runs each of these through the session, so a step cannot show code the
    /// interpreter would refuse.
    snippet: &'static str,
    hint: &'static str,
}

/// The tour itself, in order, and `:tutorial` prints every step.
///
/// The last step names [`ReplCommand::Example`], because `:examples` is where
/// the longer catalogue lives and a tour that ends without saying so sends a
/// newcomer away from the thing they were about to want.
const TUTORIAL: &[TutorialStep] = &[
    TutorialStep {
        title: "A value, and a name for it",
        body: "Redblue reads as English. `set x to 5` gives 5 the name x.",
        snippet: "set name to \"World\"\nsay name",
        hint: "Type `set name to \"World\"`, then `say name` — the name answers \
               with the value it was given.",
    },
    TutorialStep {
        title: "Arithmetic",
        body: "Numbers do what numbers do. Parentheses group, as they always do.",
        snippet: "set x to (10 + 5) * 2\nsay x",
        hint: "Type `set x to (10 + 5) * 2` and then `say x`. It answers 30.",
    },
    TutorialStep {
        title: "A condition",
        body: "`if … then … else … end` chooses between two paths.",
        snippet: "set age to 30\nif age is greater than 18 then\n    say \"Adult\"\nelse\n    say \"Minor\"\nend",
        hint: "Change 30 to 12 and run it again — the other branch is taken.",
    },
    TutorialStep {
        title: "A loop",
        body: "Blocks end with `end`. Everything indented inside them runs once per turn.",
        snippet: "repeat 3 times\n    say \"Hello!\"\nend",
        hint: "Change 3 to 1. The block still needs its `end`, however short it is.",
    },
    TutorialStep {
        title: "A function",
        body: "`to name(args) … end` defines one. Its body is what the name does.",
        snippet: "to greet(who)\n    say \"Hello, \" + who + \"!\"\nend\ngreet(\"World\")",
        hint: "Call it again with a different name: `greet(\"friend\")`.",
    },
    TutorialStep {
        title: "Where to go next",
        body: "Type `json.` and press TAB: the REPL offers the members of a module. \
               Type `(` after a name to see its shape. For more, `:examples`.",
        snippet: "set record to json.parse(\"{\\\"a\\\": 1}\")\nsay record",
        hint: "`:examples` shows a longer catalogue, and `:help` lists every \
               command. Both are one word away.",
    },
];

/// The lines `:tutorial` prints: a heading and one numbered step each.
///
/// A function of a [`Style`] rather than a `println!` in the middle of the
/// command dispatch, so what a reader is shown is something a test can hold —
/// including with colour forced on, which is the half that is easy to get wrong:
/// the numbering has to survive being painted.
fn tutorial_lines(style: &Style) -> Vec<String> {
    let mut lines = vec![style.paint(
        Paint::Heading,
        "Redblue tutorial — type each snippet, or ask for a hint with :hint",
    )];
    lines.push(String::new());
    for (index, step) in TUTORIAL.iter().enumerate() {
        lines.push(format!("  {}. {}", index + 1, step.title));
        lines.push(format!("     {}", step.body));
        for snippet in step.snippet.lines() {
            lines.push(format!("     > {snippet}"));
        }
        lines.push(String::new());
    }
    lines.push("  Stuck on a step? Type :hint for the step you are on.".to_string());
    lines
}

/// What `.hint` prints: the step the session is on, or how to start one.
///
/// The second answer is the one that used to be missing entirely — a session that
/// was not in a tour said nothing at all, which reads as a command that does
/// nothing rather than a command with nothing to say.
fn hint_lines(step: Option<usize>, style: &Style) -> Vec<String> {
    // The index is the number the message needs and the borrow is the step it is
    // about, so both come out of the one lookup. Recovering the index from the
    // step afterwards would be a search that can fail — and whose failure has to
    // be answered with something, which is how a step that is not in the table
    // ends up announced as step one.
    match step.and_then(|index| TUTORIAL.get(index).map(|listed| (index, listed))) {
        Some((index, step)) => vec![style.paint(
            Paint::Heading,
            &format!(
                "Hint for step {} of {}: {}",
                index + 1,
                TUTORIAL.len(),
                step.hint
            ),
        )],
        None => vec![style.paint(
            Paint::Heading,
            "There is no tutorial step in progress — type :tutorial to start one, \
             and :examples for a catalogue of what Redblue can do.",
        )],
    }
}

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
    /// The debug sub-REPL, while one is open — see [`debug_repl`].
    ///
    /// `Some` from the `.debug` that opened it until the `.back` (or the end of
    /// input) that closed it. While it is, every typed line belongs to the
    /// sub-REPL: it is dispatched by [`Repl::debug_line`] and reaches neither
    /// this session's history, nor its transcript, nor its bindings.
    debug_repl: Option<debug_repl::DebugRepl>,
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
    /// Every line this session ran and everything it said, which is what
    /// `.export` writes and what the terminal is not.
    ///
    /// The history is not this and cannot be: it holds every line *typed*,
    /// commands included, and holds no output at all. Sharing a session meant
    /// copy-pasting scrollback — prompts, colour and banner included — because
    /// this was the only record there was.
    transcript: Transcript,
    /// What the line just run said, before it is printed.
    ///
    /// Held between the VM printing it and the session recording it, because the
    /// VM prints `say` output itself ([`Vm::run`]) and the session needs the same
    /// lines as text. Cleared at the start of every line, so a line that failed to
    /// parse — and so never reached the VM — cannot inherit the last one's output.
    last_output: Vec<String>,
    /// Which step of [`TUTORIAL`] this session is on, or `None` when it is not
    /// in the tour at all.
    ///
    /// `Option` rather than an index that means "step one": `.hint` outside a
    /// tour has to say so, and an index cannot tell "on step one" from "not in
    /// the tour".
    tutorial_step: Option<usize>,
    /// What this session has run since it entered that step, so a step whose
    /// snippet is several programs can still be recognised as it is typed.
    ///
    /// `set name to "World"` and `say name` are each a whole program, and a
    /// session runs each as it is typed — so neither of them is step one of the
    /// tour, and the two of them together are.
    tutorial_typed: Vec<String>,
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
            debug_repl: None,
            style: Style::detect(),
            prompt_format: DEFAULT_PROMPT.to_string(),
            definitions: HashMap::new(),
            transcript: Transcript::new(),
            last_output: Vec::new(),
            tutorial_step: None,
            tutorial_typed: Vec::new(),
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
        // Inside the sub-REPL, completion and every other reader of this list
        // are the sub-REPL's: the names a line may be analysed against are
        // there plus what has already been bound on either side of `.back`.
        if let Some(debug) = &self.debug_repl {
            names.extend(debug.session_names());
        }
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
                    // A block the sub-REPL was holding is the sub-REPL's to
                    // report: the parent's buffer never saw it.
                    if let Some(debug) = self.debug_repl.as_mut() {
                        for answer in debug.report_unclosed_block() {
                            println!("{}", answer);
                        }
                    }
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

            // Inside the debug sub-REPL every line — command or code, buffered
            // or run whole — belongs to the sub-session and is dispatched by
            // it. It is taken before the history push below for exactly that
            // reason: what was typed at a `debug>` prompt was typed into the
            // sub-REPL, and a `:history` on the far side of `.back` listing it
            // would be claiming the parent session ran lines it never saw.
            if self.debug_repl.is_some() {
                for answer in self.debug_line(input) {
                    println!("{}", answer);
                }
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
        // The sub-REPL's prompt, while one is open: the block numbering is its
        // own, because the block is its own — `..  2` inside a sub-REPL says
        // line 2 of the block being typed there, not of any parent buffer.
        if let Some(debug) = &self.debug_repl {
            return debug.prompt();
        }
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
            ReplCommand::Export(path) => {
                self.export_session(path.as_deref());
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
            ReplCommand::Debug(argument) => {
                for answer in self.enter_debug(&argument) {
                    println!("{}", answer);
                }
                true
            }
            ReplCommand::Watch(_)
            | ReplCommand::Stack
            | ReplCommand::Locals
            | ReplCommand::Back => {
                println!("{}", debug_repl::NO_DEBUG_SESSION);
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
            ReplCommand::Pretty(expression) => {
                self.print_pretty(&expression);
                true
            }
            ReplCommand::Json(expression) => {
                self.print_json(&expression);
                true
            }
            ReplCommand::Table(expression) => {
                self.print_table(&expression);
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
            ReplCommand::Tutorial => {
                self.show_tutorial();
                true
            }
            ReplCommand::Hint => {
                self.show_hint();
                true
            }
            ReplCommand::Edit => {
                self.edit_buffer();
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
        if Self::exceeds_block_limits(self.multiline_buffer.len(), buffered.len()) {
            self.report_buffered_block_too_large();
            return;
        }
        if Self::needs_continuation(&buffered) {
            self.in_multiline = true;
            return;
        }

        let code = std::mem::take(&mut self.multiline_buffer).join("\n");
        self.in_multiline = false;
        // A failure was reported in full by the run itself, with the line that
        // failed and a caret under it; a line the user typed has nothing to add
        // to that and nowhere else to put it.
        let _ = self.execute_code(&code);
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
    ///
    /// So is the tour, though, and that is the newer half of the same question. A
    /// step is taken by running its snippet and a snippet needs the bindings the
    /// step before it bound, so a session that has been reset mid-tour is a
    /// session holding a hint for a step of a program it no longer has: `set name
    /// to "World"`, `:reset`, and `:hint` answers as though `name` were about to
    /// be asked for. A reset forgets what it was, and where in the tour it was is
    /// part of what it was.
    fn reset_session(&mut self) {
        self.vm = Vm::new();
        self.variables.clear();
        self.definitions.clear();
        self.multiline_buffer.clear();
        self.in_multiline = false;
        self.tutorial_step = None;
        self.tutorial_typed.clear();
        // The transcript with them: an export of a session that had forgotten the
        // lines it ran would claim a session that no longer exists ran them.
        self.transcript.clear();
        println!("REPL state reset.");
    }

    /// Whether a buffer of `bytes` over `lines` is too much for a block the user
    /// is still typing.
    ///
    /// Both limits, because either alone leaves a hole: a block of a thousand
    /// blank lines is small in bytes, and one line of a megabyte is one line.
    ///
    /// A count rather than the lines themselves, because `.edit` has a block too
    /// and holds it as the one string an editor wrote.
    fn exceeds_block_limits(lines: usize, bytes: usize) -> bool {
        lines > MAX_BLOCK_LINES || bytes > MAX_BLOCK_BYTES
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

    /// Runs `code` as the session's own line, and what it answered.
    ///
    /// The `Err` carries the failure already reported in full — with its `-->`,
    /// its offending line and its caret — so a caller that needs to know whether
    /// the run worked has it without a second rendering of the same error.
    fn execute_code(&mut self, code: &str) -> Result<Value, String> {
        let offset = self.current_line_number().saturating_sub(1);
        self.execute_code_from(introspect::SESSION_FILE, offset, code)
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
    /// plus one, and hands back what it answered or what it failed with.
    ///
    /// The offset is what makes one definition format serve two callers: a line
    /// the user typed has no line of its own but has a place in the history, and
    /// a file's line *is* its line.
    fn execute_code_from(
        &mut self,
        file: &str,
        line_offset: usize,
        code: &str,
    ) -> Result<Value, String> {
        // A line that fails to parse never reaches the VM, so the output of the
        // line before it has to be out of the way before the next line starts.
        self.last_output.clear();
        let result = self.run_code(code);
        // What the program said, whatever path it left by. Taken before the
        // `= value` line is added, so the transcript records the program's own
        // output and the session's echo of it separately, as it prints them.
        let mut output = std::mem::take(&mut self.last_output);

        let outcome = match result {
            Ok(value) => {
                self.record_definitions(file, line_offset, code);
                if !matches!(value, Value::Nothing) {
                    let line = format!("= {}", value);
                    println!("{}", self.style.paint(Paint::Result, &line));
                    self.variables.insert("_".to_string(), value.clone());
                    output.push(line);
                }
                Ok(value)
            }
            Err(e) => {
                let rendered = format!("Error: {}", render_error(&self.style, code, file, &e));
                println!("{rendered}");
                output.push(rendered.clone());
                Err(rendered)
            }
        };
        // A step of the tour is taken by running it, so this is where the tour
        // moves — and only here, where the run is known to have worked.
        if outcome.is_ok() {
            self.advance_tutorial_step(code);
        }
        // Recorded as plain text: what the session painted for a reader is not
        // what a document should carry, so the escape sequences are taken off
        // here rather than at the far end of the export.
        self.transcript.record(
            code.to_string(),
            output.iter().map(|line| strip_escapes(line)).collect(),
        );
        outcome
    }

    /// Moves a session in a tour on to the step after the one it just ran.
    ///
    /// A step is taken by running its snippet and by nothing else, and a snippet
    /// arrives one run at a time rather than in one piece: `set name to "World"`
    /// is a whole program and so is `say name`, so a reader who types step one
    /// exactly as printed runs two programs and neither of them is the step. What
    /// has been run since the step began is therefore accumulated, and the step is
    /// taken when the accumulation *is* the snippet: a prefix of it means the
    /// reader is part-way through and is still on this step, and anything else
    /// means this was something of their own, so the next attempt at the snippet
    /// starts from the beginning.
    ///
    /// Only runs that worked get here — the caller asks after a successful one —
    /// so a step the reader typed with a typo in it is a step they have not taken,
    /// and its hint is the one they get.
    ///
    /// The last step leaves no step behind it, so a finished tour is over rather
    /// than parked on its final hint.
    fn advance_tutorial_step(&mut self, code: &str) {
        let Some(index) = self.tutorial_step else {
            return;
        };
        let Some(step) = TUTORIAL.get(index) else {
            self.tutorial_step = None;
            return;
        };

        let snippet = step.snippet.trim();
        self.tutorial_typed.push(code.trim().to_string());
        let typed = self.tutorial_typed.join("\n");

        if typed == snippet {
            self.tutorial_step = index.checked_add(1).filter(|next| *next < TUTORIAL.len());
            self.tutorial_typed.clear();
        } else if !snippet.starts_with(&typed) {
            self.tutorial_typed.clear();
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
        // What the program said, kept for the transcript `.export` writes. On the
        // error path these are the lines the program printed before it failed,
        // which are printed here rather than by [`Vm::run`] — the VM gives up
        // before its print loop — and on the success path `Vm::run` has already
        // printed them and left the buffer, so this is the same lines read once
        // more. Drained either way: an export that re-printed them would be the
        // bug this is here to prevent, one line later.
        self.last_output = self.vm.take_output();
        if result.is_err() {
            for line in &self.last_output {
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
        // `.which` as `path:line`, which is a place the user can open. A file
        // that failed to run has said so already, in the same rendered error a
        // typed line would have produced.
        let _ = self.execute_code_from(path, 0, &source);
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

    /// Writes the session as Markdown — to `path`, or to stdout when given
    /// nothing — and says where it went.
    ///
    /// The other half of `:save`, and a different document. `:save` writes the
    /// lines the session *typed*, one per line, so a session can be restored; this
    /// writes what the session *ran* and what it said, so a reader who was not at
    /// the terminal can read it. Neither is the other: a transcript of inputs and
    /// no outputs is a log, and the scrollback it replaces carried the prompts and
    /// the colour codes as well.
    ///
    /// The destination is part of what the command means, so the message names it
    /// and a failure names the path that failed — "No such file or directory"
    /// alone does not say which of the paths in the command was at fault.
    fn export_session(&self, path: Option<&str>) {
        let document = self.transcript.markdown();
        match path {
            None => println!("{}", document.trim_end()),
            Some(path) => match std::fs::write(path, &document) {
                Ok(()) => println!(
                    "Exported {} exchange(s) to '{}'.",
                    self.transcript.len(),
                    path
                ),
                Err(e) => println!("Could not export session to '{}': {}", path, e),
            },
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
        let value = self.value_answer(code, ":type")?;
        let definition = self.definition_of_expression(code);
        Ok(introspect::type_lines(&value, definition.as_ref()))
    }

    /// `.pretty <expression>` — the value as an indented structure.
    ///
    /// The same gates as `.type`, and for the same reasons: a question is a
    /// question, and printing a value must not change the world it is printed
    /// from. Only the shape of the answer differs.
    fn print_pretty(&mut self, code: &str) {
        match self.pretty_answer(code) {
            Ok(text) => println!("{}", text),
            Err(message) => println!("{}", self.style.paint(Paint::Error, &message)),
        }
    }

    /// What `.pretty` has to show about `code`, or why it will not show it.
    fn pretty_answer(&mut self, code: &str) -> Result<String, String> {
        let value = self.value_answer(code, ".pretty")?;
        render::pretty(&value)
    }

    /// `.json <expression>` — the value as JSON, on one line.
    ///
    /// The document the language's own `json.stringify` writes, so what the
    /// REPL shows and what a program produces are one document.
    fn print_json(&mut self, code: &str) {
        match self.json_answer(code) {
            Ok(text) => println!("{}", text),
            Err(message) => println!("{}", self.style.paint(Paint::Error, &message)),
        }
    }

    /// What `.json` has to show about `code`, or why it will not show it.
    fn json_answer(&mut self, code: &str) -> Result<String, String> {
        let value = self.value_answer(code, ".json")?;
        Ok(render::json(&value))
    }

    /// `.table <expression>` — a list of records as aligned columns.
    fn print_table(&mut self, code: &str) {
        match self.table_answer(code) {
            Ok(text) => print!("{}", text),
            Err(message) => println!("{}", self.style.paint(Paint::Error, &message)),
        }
    }

    /// What `.table` has to show about `code`, or why it will not show it.
    fn table_answer(&mut self, code: &str) -> Result<String, String> {
        let value = self.value_answer(code, ".table")?;
        render::table(&value)
    }

    /// The value `code` names, or why there is none to name.
    ///
    /// The three gates every question about a value goes through, in the order
    /// each of them was needed:
    ///
    /// 1. **The program must be one expression.** [`introspect::expression_of_for`].
    /// 2. **No call may leave the value system.** [`Repl::effectful_call_in`].
    /// 3. **It runs on a fork.** [`Repl::evaluate_for_question`].
    ///
    /// `command` is the name the refusal quotes, so a user who typed `.pretty`
    /// is not told about `:type`.
    fn value_answer(&mut self, code: &str, command: &str) -> Result<Value, String> {
        let program = parse_only(code).map_err(|error| failure_line(&self.style, code, &error))?;
        let expression = introspect::expression_of_for(&program, command)?;
        if let Some(call) = self.effectful_call_in(expression) {
            return Err(format!(
                "Error: {command} is a question about a value, and {} does something outside \
                 the value it answers — a question cannot undo it. `:sighelp {}` describes it.",
                call, call
            ));
        }
        self.evaluate_for_question(&program, code)
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

    /// Prints [`TUTORIAL`], the guided tour a newcomer is walked through, and
    /// leaves the session on its first step so `.hint` has a step to answer for.
    ///
    /// From the first step again, wherever the reader had got to: a tour is a
    /// list of six things to try, and asking for it is asking for the list.
    fn show_tutorial(&mut self) {
        for line in tutorial_lines(&self.style) {
            println!("{line}");
        }
        self.tutorial_step = Some(0);
        self.tutorial_typed.clear();
    }

    /// Prints the hint for the step the session is on, or says how to start one.
    fn show_hint(&self) {
        for line in hint_lines(self.tutorial_step, &self.style) {
            println!("{line}");
        }
    }

    /// The editor the user named, from `$EDITOR`.
    ///
    /// `None` when it is unset or empty. An empty `EDITOR=` is the same as no
    /// editor at all, and spawning `""` would be a failure with a message about a
    /// program that does not exist rather than one about the variable the reader
    /// has to set.
    fn editor_program() -> Option<String> {
        std::env::var("EDITOR")
            .ok()
            .map(|editor| editor.trim().to_string())
            .filter(|editor| !editor.is_empty())
    }

    /// The program `.edit` came back with, or the message saying why there is
    /// none — and either way the session keeps whatever it was holding.
    ///
    /// `editor` is the program to run rather than the environment read here,
    /// because the environment is process-wide and a test cannot own it: the
    /// command passes [`Self::editor_program`], and a test passes a script it
    /// wrote, so the round trip is pinned without a real terminal editor and
    /// without one test's `$EDITOR` reaching another's.
    fn edit_in(&self, editor: Option<&str>, scratch_dir: &Path) -> Result<String, String> {
        let Some(editor) = editor else {
            return Err(
                ".edit has no editor to open: set EDITOR to the program you edit \
                 with, for example `export EDITOR=vi`"
                    .to_string(),
            );
        };
        let mut parts = editor.split_whitespace();
        let Some(program) = parts.next() else {
            return Err(format!("`{editor}` is not a program that can be run"));
        };
        let mut command = std::process::Command::new(program);
        command.args(parts);

        let scratch_dir = self.private_scratch_dir(scratch_dir)?;
        let scratch = scratch_dir.path().join("redblue-edit.rb");
        self.write_scratch_file(&scratch)?;

        // From here the scratch file holds the session's own block, and
        // `scratch_dir` goes out of scope on every way out of this function — a
        // return, a `?`, a panic. An edit that failed three times in a row must
        // not leave three copies of what the reader was writing in the temp
        // directory, and a removal written at the end of each path is one of
        // those paths getting it wrong.
        let status = command
            .arg(&scratch)
            .status()
            .map_err(|error| format!("could not run `{program}`: {error}"))?;
        if !status.success() {
            return Err(format!(
                "`{program}` exited with {status}; nothing was read back from it"
            ));
        }

        let edited = self.read_edited_block(&scratch, program)?;
        if edited.trim().is_empty() {
            return Err("the editor left the file empty, so there is nothing to run".to_string());
        }
        Ok(edited)
    }

    /// What `$EDITOR` saved, under the same limit a typed block is held to.
    ///
    /// `.edit` is a second way into the session and the block a session holds was
    /// never a rule about the keyboard: a runaway editor — a macro that filled a
    /// buffer, a `>` where a `s` was meant — wrote a file that no typed block
    /// could have produced, and it was read whole and parsed.
    ///
    /// The read is capped rather than the answer: `take` stops one byte past the
    /// limit, so a file of any size is a read of at most [`MAX_BLOCK_BYTES`] + 1
    /// bytes and the rest is never in memory. That byte over is what says "there
    /// was more than the limit", so a block exactly at it is still a block the
    /// session runs — the same boundary the typed path draws at `>`.
    fn read_edited_block(&self, path: &Path, program: &str) -> Result<String, String> {
        use std::io::Read;

        let unreadable = |error: std::io::Error| -> String {
            if error.kind() == std::io::ErrorKind::InvalidData {
                format!("what `{program}` saved to {} is not text", path.display())
            } else {
                format!(
                    "could not read back what `{program}` saved to {}: {error}",
                    path.display()
                )
            }
        };
        let mut block = String::new();
        std::fs::File::open(path)
            .map_err(unreadable)?
            .take(MAX_BLOCK_BYTES as u64 + 1)
            .read_to_string(&mut block)
            .map_err(unreadable)?;

        if Self::exceeds_block_limits(block.lines().count(), block.len()) {
            return Err(format!(
                "the block `{program}` saved is past what a session holds for one \
                 block — at most {MAX_BLOCK_LINES} lines or {MAX_BLOCK_BYTES} bytes — \
                 so nothing was read back from it"
            ));
        }
        Ok(block)
    }

    /// A directory under `parent` that only this user can reach, and the one the
    /// editor is opened over.
    ///
    /// `/tmp` is a world-writable directory that every local user shares, so a
    /// file written straight into it as `redblue-edit-<pid>.rb` is a name another
    /// user can predict — the process id is a small number — and pre-create as a
    /// symlink. `std::fs::write` follows such a link and truncates what it points
    /// at, so the buffer would have been written through it into a file the
    /// reader can write, and that path handed to `$EDITOR`.
    ///
    /// A name that is taken is a name this session does not write into, and a
    /// name that is free is not reused either — it is taken, not borrowed.
    /// `create_dir` is `mkdir`, not `mkdir -p`, so a directory planted under a
    /// serial with someone else's permissions is reported as taken and the search
    /// moves on; and because it was never entered, nothing of the session's was
    /// ever written inside it. The search runs [`SCRATCH_DIR_ATTEMPTS`] serials
    /// before it reports that it could not have a directory of its own, and what
    /// it reports then is the whole range it tried, not a generic failure.
    ///
    /// Nothing inside a directory it did make is reachable by anyone else, so
    /// nothing inside it can have been planted by anyone else either.
    fn private_scratch_dir(&self, parent: &Path) -> Result<ScratchDir, String> {
        std::fs::create_dir_all(parent).map_err(|error| {
            format!("could not make a scratch directory in {parent:?}: {error}")
        })?;
        for serial in 0..SCRATCH_DIR_ATTEMPTS {
            let dir = parent.join(format!("redblue-edit-{}-{serial}", std::process::id()));
            match std::fs::create_dir(&dir) {
                Ok(()) => {
                    // Owned from here, and before it is locked down: a
                    // `chmod` that failed is a failure this session made the
                    // directory for, so the directory goes with it.
                    let owned = ScratchDir::new(dir);
                    Self::lock_down_dir(owned.path())?;
                    return Ok(owned);
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(format!(
                        "could not make {} for an edit: {error}",
                        dir.display()
                    ))
                }
            }
        }
        Err(format!(
            "could not make a directory of its own for an edit under {parent:?}: \
             every name from redblue-edit-{}-0 to redblue-edit-{}-{} was taken",
            std::process::id(),
            std::process::id(),
            SCRATCH_DIR_ATTEMPTS - 1
        ))
    }

    /// Takes away everyone else's access to `dir`, whatever the umask left of it.
    ///
    /// `create_dir` asks the kernel for `0o777 & ~umask`, so a directory made in a
    /// shared `/tmp` is normally `0755` — listable by every local user for as
    /// long as the editor is open. The file inside it is `0600` regardless, so
    /// what is private is the buffer; the directory is closed as well so that
    /// what a reader is working on is not announced by a name they did not choose.
    #[cfg(unix)]
    fn lock_down_dir(dir: &Path) -> Result<(), String> {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("could not make {} private: {error}", dir.display()))
    }

    #[cfg(not(unix))]
    fn lock_down_dir(_dir: &Path) -> Result<(), String> {
        // No mode to set here. What the file inside it was created with is still
        // this platform's own answer to who may read it.
        Ok(())
    }

    /// Writes the session's block to `path`, for an editor to open.
    ///
    /// `create_new` rather than `write`: the file is created by this call or not
    /// at all, so a symlink sitting at that path is a failure rather than
    /// something the buffer is written through. And `0o600` rather than whatever
    /// the umask leaves behind — the block is the reader's own, and for as long
    /// as the editor is open it does not need to be readable by anyone else.
    fn write_scratch_file(&self, path: &Path) -> Result<(), String> {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(path)
            .map_err(|error| format!("could not write {}: {error}", path.display()))?;
        std::io::Write::write_all(&mut file, self.multiline_buffer.join("\n").as_bytes())
            .map_err(|error| format!("could not write {}: {error}", path.display()))
    }

    /// `.edit` over the block this session is holding, with the editor and the
    /// directory given rather than read from the process.
    ///
    /// This is the entry point the command uses and the one a test drives, so
    /// what is asserted afterwards — the buffer that survived a failed edit, the
    /// block that came back and ran — is asserted about the session's own state
    /// and not about a scratch file's leftovers.
    fn edit_with(&mut self, editor: Option<&str>, scratch_dir: &Path) -> Result<(), String> {
        let edited = self.edit_in(editor, scratch_dir)?;
        self.multiline_buffer.clear();
        self.in_multiline = false;
        match self.execute_code(&edited) {
            Ok(_) => Ok(()),
            Err(_) => {
                // The editor's block is the only copy of it there is, and the
                // buffer was cleared before the run on purpose — a block that ran
                // must not be left to be run again by the next line typed. A
                // block that did not run is the other case: clearing it first
                // threw away a typo at the end of a long program with no way back
                // into it, and `.edit` is the only route into the session for
                // anything longer than a line.
                self.multiline_buffer = edited.lines().map(str::to_string).collect();
                self.in_multiline = Self::needs_continuation(&edited);
                Err(
                    "nothing was run: the block the editor saved is back in the \
                     buffer — `.edit` to try again, or keep typing"
                        .to_string(),
                )
            }
        }
    }

    /// Opens `$EDITOR` over the block this session is holding, runs what comes
    /// back, and keeps everything when the editor could not be opened at all.
    fn edit_buffer(&mut self) {
        let editor = Self::editor_program();
        let scratch = std::env::temp_dir();
        if let Err(message) = self.edit_with(editor.as_deref(), &scratch) {
            println!(
                "{}",
                self.style.paint(Paint::Error, &format!("Error: {message}"))
            );
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
                println!("    {}", line);
            }
            println!();
        }
    }
}

/// A directory this session made for one `.edit`, and the thing that takes it
/// away again.
///
/// The removal is in `drop`, not at the end of the edit, because an edit has
/// more ways out than the one it succeeds by: the editor is not there, it exits
/// badly, its save cannot be read back, the block it saved is past what a
/// session holds. A removal written on each of those is a list to keep in step
/// with the paths above it, and a list is exactly what grows a new path and
/// forgets it. Owned rather than borrowed, so holding this is what makes the
/// directory exist: nobody can reach the file inside without going through it.
#[derive(Debug)]
struct ScratchDir(PathBuf);

impl ScratchDir {
    /// Makes `path` a directory this session owns.
    fn new(path: PathBuf) -> Self {
        ScratchDir(path)
    }

    /// The directory the editor is opened over.
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        // Whether this succeeds is not something to report over an edit that
        // already failed: the block the user was writing is in the session, and
        // this is the temporary directory, which the operating system reclaims
        // anyway.
        let _ = std::fs::remove_dir_all(&self.0);
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

    /// A path under `target/tmp/` that nothing has written to.
    ///
    /// For the file a call *would* have written if it were allowed to write
    /// it: the assertion is that the file is not there afterwards, so the path
    /// has to be one that is not there to begin with. The serial keeps two
    /// tests running in parallel from sharing a path.
    fn unwritten_path(name: &str) -> PathBuf {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let serial = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/tmp/repl-debug-watch");
        std::fs::create_dir_all(&dir).expect("scratch directory should be creatable");
        let path = dir.join(format!("{}-{}.txt", name, serial));
        let _ = std::fs::remove_file(&path);
        path
    }

    /// A directory under `target/tmp/` for a test that needs one, named after
    /// the test so two running in parallel cannot collide.
    fn scratch_dir(name: &str) -> PathBuf {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let serial = COUNTER.fetch_add(1, Ordering::SeqCst);
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/tmp/repl-debug-dirs");
        let dir = base.join(format!("{}-{}", name, serial));
        std::fs::create_dir_all(&dir).expect("scratch directory should be creatable");
        dir
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

    /// The sub-REPL is a session *inside* the session, and what `.back` owes
    /// the user is the session that was left: every piece of parent-owned state
    /// byte-identical, and one parent line typed after `.back` running in the
    /// parent rather than in the sub-REPL.
    ///
    /// This drives the real read loop through a sub-REPL that binds a name,
    /// registers a watch, opens and closes a block, inspects frames and
    /// returns — then checks the history (which lines the parent believes were
    /// typed at its prompt — and it is every sub-REPL line that must *not* be
    /// there), the transcript, the definitions, the VM's bindings, and the
    /// line buffer the `repeat` block could have leaked into.
    #[test]
    fn a_debug_sub_repl_leaves_the_parent_session_untouched() {
        let mut repl = Repl::new();
        repl.execute_line("set parent_value to 42");
        let transcript_before = repl.transcript.markdown();

        let mut source = ScriptedSource::new(vec![
            Ok(Line::Text(".debug".to_string())),
            Ok(Line::Text("set scratch_value to 99".to_string())),
            Ok(Line::Text(".watch parent_value".to_string())),
            Ok(Line::Text(".stack".to_string())),
            Ok(Line::Text(".locals".to_string())),
            Ok(Line::Text("repeat 2 times".to_string())),
            Ok(Line::Text(
                "set scratch_value to scratch_value + 1".to_string(),
            )),
            Ok(Line::Text("end".to_string())),
            Ok(Line::Text("99 + 2".to_string())),
            Ok(Line::Text(".back".to_string())),
            Ok(Line::Text("set resumed to 1".to_string())),
            Ok(Line::Eof),
        ]);
        drive(&mut repl, &mut source);

        let history: Vec<String> = (0..repl.history.len())
            .filter_map(|index| repl.history.get(index).cloned())
            .collect();
        assert_eq!(
            history,
            [".debug".to_string(), "set resumed to 1".to_string()],
            "the parent's history is the lines typed at the parent prompt, and \
             none of the sub-REPL's"
        );

        let transcript = repl.transcript.markdown();
        assert!(
            transcript.starts_with(&transcript_before),
            "everything the parent's transcript held before the sub-REPL opened \
             is still there, first and unchanged"
        );
        assert!(
            !transcript.contains("scratch_value")
                && !transcript.contains(".watch")
                && !transcript.contains("repeat 2 times"),
            "nothing the sub-REPL ran or was asked belongs in the parent's \
             transcript"
        );
        assert!(
            transcript.contains("set resumed to 1"),
            "and the line typed after .back is recorded as the parent's own"
        );

        assert!(
            !repl.definitions.contains_key("scratch_value")
                && repl.definitions.contains_key("resumed"),
            "the parent records what the parent bound: resumed yes, the \
             sub-REPL's scratch_value never"
        );
        assert_eq!(
            repl.vm.user_value("parent_value").map(ToString::to_string),
            Some("42".to_string()),
            "the binding the parent session had is still there, unchanged"
        );
        assert_eq!(
            repl.vm.user_value("scratch_value"),
            None,
            "and the names the sub-REPL bound are not in the parent's VM"
        );
        assert_eq!(
            repl.vm.user_value("resumed").map(ToString::to_string),
            Some("1".to_string()),
            "the line typed after .back ran in the parent's VM"
        );

        assert!(
            repl.debug_repl.is_none(),
            ".back closed the sub-REPL: the resumed line reached the parent"
        );
        assert!(
            repl.multiline_buffer.is_empty() && !repl.in_multiline,
            "the block typed inside the sub-REPL never touched the parent's \
             line buffer"
        );
        assert!(
            !repl.variables.contains_key("_"),
            "the sub-REPL's last result — 99 + 2 answered 101 there — never \
             became the parent's _"
        );
        assert!(
            source.prompts().iter().any(|prompt| prompt == "debug> "),
            "the sub-REPL's lines were read at the sub-REPL's prompt, which is \
             what put them outside the parent to begin with"
        );
    }

    use super::debug_repl::{read_source, BANNER, NO_DEBUG_SESSION};

    /// A file run *inside* the sub-REPL is watched like anything else — which
    /// is the reason `.run` is a sub-REPL command and not only the session's.
    ///
    /// A file run at `.debug` time is run before there is a watch to see it, so
    /// the only way to watch a file is to run it after the watches are set. The
    /// file below adds one to `x` three times; the watch was set on the `x` the
    /// parent session already held, so the first stop takes its baseline and
    /// every later change is a report.
    #[test]
    fn debug_a_file_can_be_watched_while_it_runs() {
        let mut repl = Repl::new();
        repl.execute_line("set x to 0");
        let path = scratch_file(
            "watched",
            "set x to x + 1\nset x to x + 1\nset x to x + 1\n",
        );
        let run = format!(".run {}", path.display());

        let answers = repl.debug_session(&[".debug", ".watch x", &run, ".back"]);

        let reports: Vec<&str> = answers
            .iter()
            .map(String::as_str)
            .filter(|line| line.starts_with("watch x = "))
            .collect();
        assert_eq!(
            reports,
            ["watch x = 1", "watch x = 2", "watch x = 3"],
            "every change the file made was reported, and nothing else was; \
             it answered:\n{}",
            answers.join("\n")
        );
    }

    /// A file the sub-REPL is asked to run and cannot read is one reported
    /// failure — and the session that asked for it is still a session.
    ///
    /// The failure is asserted on the read itself and not only on the wording
    /// of the message: the message is a report, the read is the failure.
    #[test]
    fn edge_a_run_of_a_file_that_cannot_be_read_reports_the_failure_and_the_session_goes_on() {
        let mut repl = Repl::new();
        let missing = "definitely_missing_phase062.rb";

        let answers =
            repl.debug_session(&[".debug", ".run definitely_missing_phase062.rb", ".stack"]);

        assert!(
            read_source(missing).is_err(),
            "the file really is not there: everything above rests on the read \
             failing rather than on the wording of a message"
        );
        assert!(
            answers
                .iter()
                .any(|line| line.starts_with("Error: cannot read 'definitely_missing_phase062.rb'")),
            "the failed read is reported with the file it wanted; it answered:\n{}",
            answers.join("\n")
        );
        assert!(
            answers.iter().any(|line| line == "No live frames."),
            "and the session is still a session: .stack answers inside it — \
             it answered:\n{}",
            answers.join("\n")
        );
    }

    /// A file whose source does not parse is a reported failure, and the
    /// session that ran it is still a session.
    ///
    /// The file is malformed on purpose — `set x to` with nothing after it —
    /// and the failure is asserted on the parse as well as on the message.
    #[test]
    fn edge_a_run_of_a_file_that_does_not_parse_reports_the_failure_and_the_session_goes_on() {
        let mut repl = Repl::new();
        let path = scratch_file("malformed", "set x to\n");
        let run = format!(".run {}", path.display());

        let answers = repl.debug_session(&[".debug", &run, ".stack"]);

        assert!(
            crate::repl::parse_only("set x to").is_err(),
            "the file really is malformed: everything above rests on the parse \
             failing rather than on the wording of a message"
        );
        assert!(
            answers.iter().any(|line| line.starts_with("Error:")),
            "the malformed file is reported as a failure; it answered:\n{}",
            answers.join("\n")
        );
        assert!(
            answers.iter().any(|line| line == "No live frames."),
            "and the session that ran it is still a session: .stack answers \
             inside it — it answered:\n{}",
            answers.join("\n")
        );
    }

    /// A file is a file whatever it is called: the suffix is the fallback for a
    /// path that is not there, not the only way in.
    ///
    /// The file here is called `.txt`, and holds Redblue source. Before the
    /// suffix rule was the only rule, `.debug <this file>` was parsed as code
    /// and failed on the dot.
    #[test]
    fn debug_a_file_whose_name_is_not_rb_is_read_when_it_exists() {
        let mut repl = Repl::new();
        let path = scratch_path("debug-source", "txt");
        std::fs::write(&path, "set y to 41\ny + 1\n").expect("the scratch file should be writable");
        let argument = path.display().to_string();

        let answers = repl.debug_session(&[&format!(".debug {argument}"), ".back"]);

        assert!(
            answers.iter().any(|line| line == "= 42"),
            "the file was read and run, not parsed as an expression; it answered:\n{}",
            answers.join("\n")
        );
    }

    /// `.watch` with no expression is a question about the watches the session
    /// holds, and is answered with them — a session that holds none says so.
    ///
    /// Three shapes are pinned here: a session with no watch at all, a session
    /// with exactly one, and a session with the same expression watched twice.
    /// The last is two watches, not one — each compares against the value it
    /// last saw, and neither shares state with the other.
    #[test]
    fn debug_watch_with_no_argument_lists_the_watches_the_session_holds() {
        let mut repl = Repl::new();

        let answers = repl.debug_session(&[
            ".debug",
            ".watch",
            ".watch x",
            ".watch",
            ".watch x",
            ".watch \"héllo\"",
            ".watch",
        ]);

        assert!(
            answers.iter().any(|line| line == "No watches."),
            "a session that holds no watch says so rather than nothing; it \
             answered:\n{}",
            answers.join("\n")
        );
        assert!(
            answers
                .iter()
                .any(|line| line == "Watching 1 expression(s):"),
            "one watch is one watch; it answered:\n{}",
            answers.join("\n")
        );
        assert!(
            answers
                .iter()
                .any(|line| line == "Watching 3 expression(s):"),
            "the same expression watched twice is two watches, and a third one \
             is a third; it answered:\n{}",
            answers.join("\n")
        );
        let final_listing = answers
            .iter()
            .rposition(|line| line == "Watching 3 expression(s):")
            .map(|index| &answers[index + 1..])
            .expect("the session listed the three watches it holds");
        let x_rows = final_listing
            .iter()
            .filter(|line| line.as_str() == "  x")
            .count();
        assert_eq!(
            x_rows,
            2,
            "the duplicate watch is listed twice, once per watch; it answered:\n{}",
            answers.join("\n")
        );
        assert!(
            final_listing
                .iter()
                .any(|line| line.as_str() == "  \"héllo\""),
            "a watch on text is listed by the text it watches, whatever the \
             text is; it answered:\n{}",
            answers.join("\n")
        );
    }

    /// A constant declared inside a frame is that frame's own binding, and
    /// `.locals` shows it.
    ///
    /// A `constant` binds a global, exactly as a `set` that reaches one does —
    /// and exactly as that `set` does, it belongs to the frame that wrote it.
    /// Before the frame's writes were tracked for constants as well as for
    /// `set`, the name was written and then never listed.
    #[test]
    fn debug_locals_shows_a_constant_the_frame_declared() {
        let mut repl = Repl::new();
        feed(
            &mut repl,
            &[
                "to inner()",
                "constant LIMIT to 5",
                "say \"inside\"",
                "end",
                "to outer()",
                "inner()",
                "end",
            ],
        );

        let answers = repl.debug_session(&[".debug outer()", ".locals", ".back"]);

        assert!(
            answers.iter().any(|line| line == "  LIMIT = 5"),
            "the constant the frame declared is one of the frame's bindings; \
             it answered:\n{}",
            answers.join("\n")
        );
    }

    /// The three-deep chain a `.stack` inside the sub-REPL must name, outermost
    /// frame first. The last statement the program steps in is `set deep to 7`,
    /// deep inside `inner`, which is the step the stack is read at.
    #[test]
    fn debug_stack_names_a_three_deep_call_chain() {
        let mut repl = Repl::new();
        feed(
            &mut repl,
            &[
                "to inner()",
                "set deep to 7",
                "end",
                "to middle()",
                "inner()",
                "end",
                "to outer()",
                "middle()",
                "end",
            ],
        );

        let answers = repl.debug_session(&[".debug outer()", ".stack", ".back"]);

        let expected = ["Call stack (3 frames):", "  outer", "  middle", "  inner"];
        let named = answers.windows(expected.len()).any(|window| {
            window
                .iter()
                .map(String::as_str)
                .eq(expected.iter().copied())
        });
        assert!(
            named,
            ".stack should name all three frames in call order; it answered:\n{}",
            answers.join("\n")
        );
    }

    /// `.watch` reporting exactly the steps where its value changed, and no
    /// others — across a loop, typed block by block the way a sub-REPL holds it.
    ///
    /// Four statements run once the watch is set — the `repeat`, then the body
    /// it runs three times — and `x` changes on three of them. The `repeat`
    /// sees the value the watch took as its baseline and must say nothing;
    /// every other step must speak.
    #[test]
    fn debug_watch_reports_only_the_steps_where_the_value_changed() {
        let mut repl = Repl::new();

        let answers = repl.debug_session(&[
            ".debug",
            "set x to 0",
            ".watch x",
            "repeat 3 times",
            "set x to x + 1",
            "end",
            ".back",
        ]);

        let reports: Vec<&str> = answers
            .iter()
            .map(String::as_str)
            .filter(|line| line.starts_with("watch x = "))
            .collect();
        assert_eq!(
            reports,
            ["watch x = 1", "watch x = 2", "watch x = 3"],
            "a watch reports each changed step once and the unchanged ones never; \
             it answered:\n{}",
            answers.join("\n")
        );
        assert!(
            answers.iter().any(|line| line == "Watching x."),
            "registering a watch says so; it answered:\n{}",
            answers.join("\n")
        );
    }

    /// `.locals` answering about the frame the program stepped in — its own
    /// bindings, sorted — and about nothing that belongs to the frames above
    /// it: the caller's local and a global the caller overwrote must both be
    /// absent while the callee runs, even though the callee could see the
    /// global through Redblue's own scoping.
    ///
    /// The step read at is the *last* one the program takes, deep inside
    /// `inner`: `set stopped to inner_var` fires before it runs, so `stopped`
    /// itself is not yet bound — locals are what the frame held *at* the stop,
    /// which is the whole point of reading them there.
    #[test]
    fn debug_locals_shows_the_stepped_frame_and_not_the_callers() {
        let mut repl = Repl::new();
        feed(
            &mut repl,
            &[
                "set shared to 1",
                "to inner(param)",
                "set inner_var to param + 1",
                "set stopped to inner_var",
                "end",
                "to outer()",
                "set outer_var to 9",
                "set shared to 2",
                "inner(10)",
                "end",
            ],
        );

        let answers = repl.debug_session(&[".debug outer()", ".locals", ".back"]);

        assert!(
            answers.iter().any(|line| line == "  param = 10"),
            "the frame's parameter should be listed; it answered:\n{}",
            answers.join("\n")
        );
        assert!(
            answers.iter().any(|line| line == "  inner_var = 11"),
            "the frame's own local should be listed; it answered:\n{}",
            answers.join("\n")
        );
        let leaked: Vec<&str> = answers
            .iter()
            .map(String::as_str)
            .filter(|line| line.starts_with("  outer_var") || line.starts_with("  shared"))
            .collect();
        assert!(
            leaked.is_empty(),
            ".locals should not show the caller's local or its global writes; \
             it also answered:\n{}",
            answers.join("\n")
        );
    }

    /// `.debug` with no argument at all: the session opens, runs nothing, and
    /// says what it is — the empty program is not an error and not a prompt
    /// that goes unanswered, and `.back` from it returns like any other.
    #[test]
    fn edge_a_bare_debug_opens_a_session_and_runs_nothing() {
        let mut repl = Repl::new();

        let answers = repl.debug_session(&[".debug", ".back", ".stack"]);

        assert_eq!(
            answers.first().map(String::as_str),
            Some(BANNER),
            "a bare .debug opens with the banner; it answered:\n{}",
            answers.join("\n")
        );
        assert_eq!(
            answers.get(1),
            Some(&NO_DEBUG_SESSION.to_string()),
            "and .back from it returned: the next line answered outside any \
             sub-REPL; it answered:\n{}",
            answers.join("\n")
        );
    }

    /// An expression that does not parse cannot be watched — registration is
    /// where that is found, before any run would be asked to observe it — and
    /// finding it must not cost the session. The sub-REPL stays open, `.stack`
    /// still answers for a session that has stepped nowhere, and `.back`
    /// returns.
    #[test]
    fn edge_a_watch_that_cannot_parse_fails_and_the_session_goes_on() {
        let mut repl = Repl::new();

        let answers = repl.debug_session(&[".debug", ".watch 1 +", ".stack", ".back"]);

        assert!(
            answers
                .iter()
                .any(|line| line.starts_with("Error: ") && line.contains("1 +")),
            "an unparseable watch is a reported failure, not a watch; it \
             answered:\n{}",
            answers.join("\n")
        );
        assert!(
            crate::repl::parse_only("1 +").is_err(),
            "the whole behaviour rests on this: `1 +` is not an expression, so \
             there is nothing a run could ever evaluate to watch it"
        );
        assert!(
            answers.iter().any(|line| line == "No live frames."),
            "and the session that refused the watch is still a session: .stack \
             answers about it — it answered:\n{}",
            answers.join("\n")
        );
        assert!(
            !answers.contains(&NO_DEBUG_SESSION.to_string()),
            "the refused watch never closed the sub-REPL; it answered:\n{}",
            answers.join("\n")
        );
    }

    /// A watch on a name nobody bound is one report per step it fails on and
    /// then another at the end of the run — never an error *in* the run. The
    /// program still finishes, the reports still print, and the session takes
    /// the next command it is given.
    #[test]
    fn edge_a_watch_on_an_unbound_name_reports_per_step_and_the_run_still_finishes() {
        let mut repl = Repl::new();

        let answers = repl.debug_session(&[
            ".debug",
            "set y to 0",
            ".watch nosuchname",
            "repeat 2 times",
            "set y to y + 1",
            "end",
            ".debug 1 + 1",
        ]);

        let failures: Vec<&str> = answers
            .iter()
            .map(String::as_str)
            .filter(|line| line.starts_with("watch nosuchname:"))
            .collect();
        // The loop's run reports four: the `repeat`, its body twice, and the
        // end-of-run pass. The `1 + 1` typed afterwards reports two: its one
        // statement, and its own end-of-run pass — the watch is still
        // registered, and a surviving session is a session still watching.
        assert_eq!(
            failures.len(),
            6,
            "a failing watch reports once per step — it answered:\n{}",
            answers.join("\n")
        );
        assert!(
            failures
                .iter()
                .all(|line| line.contains("Unknown variable 'nosuchname'")),
            "and each report names what it could not find; it answered:\n{}",
            answers.join("\n")
        );
        assert!(
            answers.iter().any(|line| line == "= 2"),
            "the program ran to its result despite the watch — it answered:\n{}",
            answers.join("\n")
        );
    }

    /// A watch on something that would change the world is *refused*, not
    /// performed — and the refusal is the whole answer, because a watch is an
    /// expression that was only being watched.
    ///
    /// The scratch VM that evaluates a watch is told it is answering a
    /// question ([`crate::interpreter::Vm::ask_questions`]), so a builtin that
    /// leaves the process — `files.write` — is refused at the call, before the
    /// write it would have made. A program that wrote a file because someone
    /// typed `.watch files.write(…)` would be a debugger doing the thing it
    /// was asked to look at.
    ///
    /// The module is imported under an alias on purpose. `import files as F`
    /// binds `F` to the module `files`, and a call through the alias has to
    /// resolve to the same builtin `files.write` reaches — which it can only do
    /// if the watch is evaluated against the language context the program built
    /// and not only against its bindings. A watch that cannot see the import
    /// answers "Module 'F' has no function 'write'", which is a complaint
    /// about the wrong thing: the write it would have made is the thing.
    ///
    /// Asserted on the file and not only on the wording of the message: "it
    /// said no" and "the file is still not there" are different claims, and
    /// only the second one is the point.
    #[test]
    fn edge_a_watch_that_would_change_the_world_is_refused_and_writes_nothing() {
        let mut repl = Repl::new();
        let path = unwritten_path("watched");
        let quoted = path.display().to_string();

        let answers = repl.debug_session(&[
            ".debug",
            "import files as F",
            &format!(".watch F.write(\"{quoted}\", \"clobbered\")"),
            "1 + 1",
            ".back",
        ]);

        let refusals: Vec<&str> = answers
            .iter()
            .map(String::as_str)
            .filter(|line| line.starts_with("watch F.write("))
            .collect();
        assert!(
            !refusals.is_empty(),
            "the watch reported the refusal rather than making the write; it \
             answered:\n{}",
            answers.join("\n")
        );
        assert!(
            refusals.iter().all(|line| line.contains("files_write")),
            "and the refusal names the builtin that would have done it: {:?}",
            refusals
        );
        assert!(
            !path.exists(),
            "the watched expression wrote nothing: {} is not there",
            path.display()
        );
        assert!(
            answers.iter().any(|line| line == "= 2"),
            "and the program it was watching still ran; it answered:\n{}",
            answers.join("\n")
        );
    }

    /// A watch can call what the program *declared* — a module's own member.
    ///
    /// The bindings a watch is evaluated against are not the whole of what a
    /// program has: a module the program declared is a namespace with a member
    /// in it, and a watch that cannot see the declaration answers "Module
    /// 'Greeter' has no function 'greet'" for a member the program published.
    ///
    /// The report is the positive claim: the watch called the member and the
    /// value it produced is the one the program would have produced. The
    /// program changes the name the member is called with, so the value the
    /// watch holds changes with it — which is what a watch reports.
    #[test]
    fn debug_a_watch_can_call_a_module_the_program_declared() {
        let mut repl = Repl::new();

        let answers = repl.debug_session(&[
            ".debug",
            "module Greeter",
            "    to greet(who)",
            "        give back \"hello \" + who",
            "    end",
            "    export all",
            "end",
            "set who to \"world\"",
            ".watch Greeter.greet(who)",
            "set who to \"there\"",
            ".back",
        ]);

        let reports: Vec<&str> = answers
            .iter()
            .map(String::as_str)
            .filter(|line| line.starts_with("watch Greeter.greet("))
            .collect();
        assert!(
            reports.contains(&"watch Greeter.greet(who) = hello there"),
            "the watch called the module's member and reported what it \
             answered; it answered:\n{}",
            answers.join("\n")
        );
        assert!(
            !reports.iter().any(|line| line.contains("has no function")),
            "and it did not answer that the module has no such member: {:?}",
            reports
        );
    }

    /// A watch can call an *object's* method, for the same reason it can call a
    /// module's member: the declaration is part of what the program has, and
    /// not part of what it has bound.
    ///
    /// The receiver of an object's method is the type's own name — `Counter`,
    /// not a variable holding one — so the watch is written the way the
    /// language writes it.
    #[test]
    fn edge_a_watch_can_call_an_object_method_the_program_declared() {
        let mut repl = Repl::new();

        let answers = repl.debug_session(&[
            ".debug",
            "object Counter",
            "    has count default 0",
            "    to bump()",
            "        set this.count to this.count + 1",
            "        give back this.count",
            "    end",
            "end",
            "set Counter.count to 5",
            ".watch Counter.bump()",
            "set Counter.count to 9",
            ".back",
        ]);

        let reports: Vec<&str> = answers
            .iter()
            .map(String::as_str)
            .filter(|line| line.starts_with("watch Counter.bump()"))
            .collect();
        assert!(
            reports.contains(&"watch Counter.bump() = 10"),
            "the watch called the object's method and reported what it \
             answered; it answered:\n{}",
            answers.join("\n")
        );
        assert!(
            !reports.iter().any(|line| line.contains("has no function")),
            "and it did not answer that there is no such method: {:?}",
            reports
        );
    }

    /// A session holds a bounded number of watches, and the one past the bound
    /// is refused rather than added.
    ///
    /// Every watch is evaluated at every statement boundary, so the number of
    /// them is a multiplier on the work the program does. A cap is what keeps
    /// that arithmetic the user's to make deliberately.
    #[test]
    fn edge_a_session_refuses_a_watch_past_its_limit() {
        let mut repl = Repl::new();
        let mut lines: Vec<String> = vec![".debug".to_string()];
        for index in 0..100 {
            lines.push(format!(".watch watched{index}"));
        }
        lines.push(".watch one_too_many".to_string());
        lines.push(".watch".to_string());
        lines.push(".back".to_string());
        let asked: Vec<&str> = lines.iter().map(String::as_str).collect();

        let answers = repl.debug_session(&asked);

        assert!(
            answers
                .iter()
                .any(|line| line.contains("holds 100 watches")),
            "the watch past the limit was refused by name; it answered:\n{}",
            answers.join("\n")
        );
        assert!(
            !answers
                .iter()
                .any(|line| line.contains("Watching one_too_many.")),
            "and it was not registered either; it answered:\n{}",
            answers.join("\n")
        );
        assert!(
            answers
                .iter()
                .any(|line| line.starts_with("Watching 100 expression(s):")),
            "and the session holds exactly the watches it may hold; it \
             answered:\n{}",
            answers.join("\n")
        );
    }

    /// A path that is *not there* is code, and is run as code.
    ///
    /// The decision is made from the one read: a path that could not be read
    /// because it does not exist is not a file the user meant, and a file the
    /// user meant is reported as one that cannot be read. The two answers
    /// cannot both be right about the same path, and the read is what settles
    /// it.
    #[test]
    fn edge_a_debug_of_a_path_that_is_not_there_is_run_as_code() {
        let mut repl = Repl::new();
        let missing = "definitely_missing_phase062.txt";

        let answers = repl.debug_session(&[&format!(".debug {missing}"), ".back"]);

        assert!(
            !answers
                .iter()
                .any(|line| line.starts_with("Error: cannot read")),
            "a path that is not there is not reported as a file that cannot be \
             read; it answered:\n{}",
            answers.join("\n")
        );
        assert!(
            answers
                .iter()
                .any(|line| line.contains("Unknown variable 'definitely_missing_phase062'")),
            "it was run as code, and the code failed the way code fails; it \
             answered:\n{}",
            answers.join("\n")
        );
    }

    /// A watch that *panics* is one failed watch, not a program that dies.
    ///
    /// The panic is caught where the watch is evaluated, so that one watch
    /// cannot take the program it watches down with it — and it is reported as
    /// the failure it is, rather than swallowed.
    #[test]
    fn edge_a_watch_that_panics_is_reported_rather_than_propagated() {
        let outcome = super::debug_repl::catching_panics(|| panic!("the watcher fell over"));

        let message = outcome.expect_err("a panic is a failure, not a value");
        assert!(
            message.contains("the watcher fell over"),
            "the panic is reported with what it said; it said: {}",
            message
        );
    }

    /// A path that is there but holds no source to read is reported as what it
    /// is, and the sub-REPL opens anyway.
    ///
    /// The `.debug` argument is read once and decided from what was read: a
    /// directory is there, and there is no source in it. Saying "cannot read"
    /// names the thing the user named; running the path as code would answer
    /// with a name nobody bound, which is a complaint about the wrong thing.
    #[test]
    fn edge_a_debug_of_a_directory_says_it_cannot_be_read_and_still_opens() {
        let mut repl = Repl::new();
        let dir = scratch_dir("a-directory");

        let answers =
            repl.debug_session(&[&format!(".debug {}", dir.display()), ".stack", ".back"]);

        assert!(
            answers
                .iter()
                .any(|line| line.starts_with("Error: cannot read '")),
            "the directory is reported as something that cannot be read; it \
             answered:\n{}",
            answers.join("\n")
        );
        assert!(
            answers.iter().any(|line| line == "No live frames."),
            "and the sub-REPL opened anyway: .stack answered inside it — it \
             answered:\n{}",
            answers.join("\n")
        );
    }

    /// `.debug nope.rb` at the parent prompt: the read fails with the file it
    /// wanted, and the session opens regardless — the error is about that run,
    /// not about having somewhere to run it.
    #[test]
    fn edge_a_debug_of_a_file_that_cannot_be_read_says_so_and_still_opens() {
        let mut repl = Repl::new();

        let answers =
            repl.debug_session(&[".debug definitely_missing_phase062.rb", ".stack", ".back"]);

        assert!(
            answers.iter().any(|line| line
                .starts_with("Error: cannot read 'definitely_missing_phase062.rb'")),
            "the failed read is reported with the file it wanted; it answered:\n{}",
            answers.join("\n")
        );
        assert!(
            answers.iter().any(|line| line == "No live frames."),
            "the sub-REPL opened anyway and .stack answered inside it — \
             it answered:\n{}",
            answers.join("\n")
        );
    }

    /// Every sub-REPL command typed with no sub-REPL open answers the same one
    /// way: how to open one. The commands are not session commands that happen
    /// to need a debug session — they have no meaning outside one, and the
    /// sentence is what tells the user where they belong.
    #[test]
    fn edge_sub_repl_commands_outside_a_session_say_how_to_open_one() {
        let mut repl = Repl::new();

        let answers = repl.debug_session(&[".stack", ".locals", ".watch x", ".back"]);

        assert_eq!(
            answers,
            vec![
                NO_DEBUG_SESSION.to_string(),
                NO_DEBUG_SESSION.to_string(),
                NO_DEBUG_SESSION.to_string(),
                NO_DEBUG_SESSION.to_string(),
            ],
            "all four answered with how to open a session; they answered:\n{}",
            answers.join("\n")
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
        repl.execute_code("set total to 3")
            .expect("a `set` of a number is a line that runs");

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
        repl.execute_code("set counter to 3")
            .expect("a `set` of a number is a line that runs");
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

        /// The two places that answer "is that a module, and what is in it" must
        /// answer it about the same names.
        ///
        /// `.doc <name>` (phase-056) lists a module's members and the completer
        /// (phase-061) offers them on `<name>.`, and both read the language's own
        /// table — which is what stops them drifting. That is worth pinning rather
        /// than asserting twice: a name the completer offers members for while
        /// `.doc` refuses it would leave a reader able to type something no
        /// listing will explain, and a module `.doc` documents that TAB never
        /// reaches would leave the finding half-fixed.
        #[test]
        fn edge_doc_and_completion_agree_about_which_names_are_modules() {
            let repl = Repl::new();
            let completer = crate::repl::completer::ReplCompleter::new();

            for module in stdlib::MODULES {
                let documented = repl
                    .module_doc_answer(module)
                    .unwrap_or_else(|error| panic!(".{module} is a module: {error}"));
                let offered = completer.complete(&format!("{module}."));
                assert!(
                    !offered.is_empty(),
                    "`{module}.` offers nothing, so the module {} .doc lists {} \
                     cannot be reached from the keyboard",
                    documented.first().map(String::as_str).unwrap_or(""),
                    offered.len()
                );
                for member in &offered {
                    let spelled = member
                        .strip_prefix(&format!("{module}."))
                        .unwrap_or_else(|| panic!("{member} is offered as a member of {module}"));
                    assert!(
                        documented.iter().any(|line| line.contains(spelled)),
                        "{member} is offered on TAB but {module}'s .doc listing does \
                         not mention it: {documented:?}"
                    );
                }
            }

            // And the refusal direction: a name that is not a module is refused by
            // `.doc` and offers no members, rather than one of them answering for
            // a name the other does not know.
            for not_a_module in ["nosuchmodule", "JSON", "json_parse"] {
                assert!(
                    repl.module_doc_answer(not_a_module).is_err(),
                    "`.doc {not_a_module}` should have been refused"
                );
                assert!(
                    completer.complete(&format!("{not_a_module}.")).is_empty(),
                    "`{not_a_module}.` offered members for a name that is not a module"
                );
            }

            // The empty name is the same refusal on the other side: `.doc` has no
            // module called nothing, and a bare `.` is not a member listing at all
            // — it is the reader asking which receivers exist.
            assert!(repl.module_doc_answer("").is_err());
            assert!(
                !completer.complete(".").is_empty(),
                "a bare dot offers the receivers, which is a different question"
            );
            assert!(
                completer
                    .complete(".")
                    .iter()
                    .all(|name| !name.contains('.')),
                "and every one of them is a bare name: {:?}",
                completer.complete(".")
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

    /// The three ways a value can be printed.
    ///
    /// Before this phase every value the REPL printed went through one shape:
    /// the single-line `Display` at `src/repl/mod.rs:402`. A two-hundred-element
    /// list, a nested record and a list of records all came out as one
    /// unreadable line, and there was no way to ask for JSON or for columns.
    ///
    /// These drive the session the way a user does: through the command
    /// dispatcher, so a command that parses and is never dispatched fails here.
    mod printing {
        use super::{feed, Repl};
        use crate::repl::commands::ReplCommand;
        use crate::value::Value;

        /// `text` as a Redblue text literal, so the language's own `json.parse`
        /// can read it back.
        ///
        /// The five escapes a Redblue text literal carries, and nothing else —
        /// a document the language cannot spell is a document the REPL must not
        /// print.
        fn as_text_literal(text: &str) -> String {
            let mut out = String::from("\"");
            for c in text.chars() {
                match c {
                    '"' => out.push_str("\\\""),
                    '\\' => out.push_str("\\\\"),
                    '\n' => out.push_str("\\n"),
                    '\r' => out.push_str("\\r"),
                    '\t' => out.push_str("\\t"),
                    other => out.push(other),
                }
            }
            out.push('"');
            out
        }

        /// The three commands answer about a value the session never bound: the
        /// argument is an expression, not a name.
        #[test]
        fn the_three_printing_commands_answer_about_an_expression() {
            let mut repl = Repl::new();

            let shown = repl
                .pretty_answer("{ a: 1, b: [2, 3] }")
                .expect("a record is showable");
            assert_eq!(
                shown, "{\n  \"a\": 1,\n  \"b\": [\n    2,\n    3\n  ]\n}",
                "one level of indentation per level of nesting"
            );

            let written = repl
                .json_answer("{ a: 1, b: [2, 3] }")
                .expect("a record is writable");
            assert_eq!(
                written, "{\"a\": 1, \"b\": [2, 3]}",
                "the document the language's own json.stringify writes"
            );

            let tabled = repl
                .table_answer("[{ name: \"ada\", age: 36 }, { name: \"bob\" }]")
                .expect("a list of records is a table");
            assert_eq!(
                tabled, "name  age\nada   36\nbob\n",
                "a header row naming the keys, and one row per record"
            );
        }

        /// What `.pretty` shows reads back as the value it came from.
        ///
        /// The round trip is through the language's own `json.parse`, not
        /// through a second implementation of the same idea: a document the
        /// language cannot read is not a document the REPL should print, and
        /// only the language's reader can say so.
        #[test]
        fn pretty_output_reads_back_as_the_value_it_came_from() {
            let mut repl = Repl::new();
            feed(
                &mut repl,
                &[
                    "set original to { name: \"ada\", tags: [\"x\", \"y\"], meta: { ok: yes, n: 1 } }",
                ],
            );

            let shown = repl
                .pretty_answer("original")
                .expect("a bound record is showable");
            let literal = as_text_literal(&shown);
            let line = format!("set read_back to json.parse({})", literal);
            feed(&mut repl, &[line.as_str()]);

            assert_eq!(
                repl.lookup("read_back").cloned(),
                repl.lookup("original").cloned(),
                "the document the REPL printed reads back as the value it came from:\n{}",
                shown
            );
        }

        /// A printing command is a question about a value, and a program is not
        /// a question: it is refused by name, and the session is left exactly as
        /// the question found it.
        #[test]
        fn edge_a_printing_command_refuses_a_program_and_the_session_never_knows_it_happened() {
            let mut repl = Repl::new();

            for (command, refusal) in [
                (
                    ".pretty",
                    repl.pretty_answer("set sneaky to 1")
                        .expect_err("a program is not a value"),
                ),
                (
                    ".json",
                    repl.json_answer("say \"printed, not said\"")
                        .expect_err("a program is not a value"),
                ),
                (
                    ".table",
                    repl.table_answer("import files")
                        .expect_err("a program is not a value"),
                ),
            ] {
                assert!(
                    refusal.starts_with("Error:"),
                    "{} is reported as the error it is: {}",
                    command,
                    refusal
                );
                assert!(
                    refusal.contains(command),
                    "{} names the command that refused it: {}",
                    command,
                    refusal
                );
            }

            assert!(
                repl.lookup("sneaky").is_none(),
                "no name was bound by a command that refused to run"
            );
        }

        /// Printing a value does not change the session. The question runs on a
        /// fork, so a function that assigns writes nothing the session can see
        /// — the half of the rule the static gates cannot reach.
        #[test]
        fn edge_a_printing_command_does_not_change_the_session() {
            let mut repl = Repl::new();
            feed(
                &mut repl,
                &[
                    "set counter to 1",
                    "to bump()\n    set counter to counter + 1\n    return counter\nend",
                ],
            );

            let shown = repl.pretty_answer("bump()").expect("a call is showable");

            assert_eq!(shown, "2", "the value the call answered");
            assert_eq!(
                repl.lookup("counter").cloned(),
                Some(Value::Number(1.0)),
                "and the session's own counter is the one it had before the question"
            );
        }

        /// Every new command is in `:help`, and offered by the completer: a
        /// command that works and is neither documented nor completable is a
        /// command nobody types.
        #[test]
        fn edge_help_names_the_three_printing_commands() {
            let help = ReplCommand::help_lines().join("\n");

            for command in ["pretty", "json", "table"] {
                assert!(
                    help.contains(&format!(":{}", command)),
                    ":help names :{}: {}",
                    command,
                    help
                );
            }

            let repl = Repl::new();
            for command in ["pretty", "json", "table"] {
                let completions = repl.complete(&format!(":{}", command));
                assert!(
                    completions.contains(&format!(":{}", command)),
                    ":{} is completed: {:?}",
                    command,
                    completions
                );
            }
        }

        /// The three commands are dispatched as commands.
        ///
        /// A command that parses and is never dispatched is a command nobody
        /// can use, and the parse test above cannot see it: the dispatcher is
        /// a second place the three names are written down. So the line is
        /// handed to the dispatcher itself here, and the one thing that tells
        /// a command from source is checked — a line run as source binds `_`,
        /// and a line run as a command binds nothing.
        #[test]
        fn the_three_printing_commands_are_dispatched_and_not_run_as_source() {
            for command in [".pretty 1 + 1", ".json 1 + 1", ".table [{ a: 1 }]"] {
                let mut repl = Repl::new();

                assert!(
                    repl.handle_command(command),
                    "{} is a command, and the dispatcher consumes it",
                    command
                );
                assert!(
                    repl.lookup("_").is_none(),
                    "{} is not run as source: a source line would have bound _",
                    command
                );
            }
        }

        /// An expression that does not parse is refused as the failure it is.
        ///
        /// The three commands ask about a value, and a line that is not an
        /// expression holds no value to ask about: the refusal says so rather
        /// than printing nothing at all.
        #[test]
        fn edge_a_printing_command_of_a_line_that_does_not_parse_is_refused() {
            let mut repl = Repl::new();

            for (command, refusal) in [
                (
                    ".pretty",
                    repl.pretty_answer("1 +")
                        .expect_err("a line that does not parse is refused"),
                ),
                (
                    ".json",
                    repl.json_answer("[1, 2")
                        .expect_err("a line that does not parse is refused"),
                ),
                (
                    ".table",
                    repl.table_answer("{ a: }")
                        .expect_err("a line that does not parse is refused"),
                ),
            ] {
                assert!(
                    refusal.starts_with("Error:"),
                    "{} refuses a line that does not parse, and says so: {}",
                    command,
                    refusal
                );
            }
        }

        /// A table of exactly one record is a table: a header row naming the
        /// keys, and one row under it.
        #[test]
        fn edge_a_table_of_one_record_names_its_keys_once() {
            let mut repl = Repl::new();

            let shown = repl
                .table_answer("[{ name: \"ada\", age: 36 }]")
                .expect("one record is a table");
            let lines: Vec<&str> = shown.trim_end_matches('\n').split('\n').collect();

            assert_eq!(
                lines,
                vec!["name  age", "ada   36"],
                "a header row and one row, and the key is named once"
            );
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

    /// The guided tour, `.hint`, and `.edit`: the three commands a newcomer needs
    /// and the REPL did not have.
    mod onboarding {
        use std::fs;
        use std::path::{Path, PathBuf};
        use std::sync::atomic::{AtomicUsize, Ordering};

        use super::{feed, Repl};
        use crate::repl::style::{Paint, Style};
        use crate::repl::{hint_lines, tutorial_lines, SCRATCH_DIR_ATTEMPTS, TUTORIAL};
        // The editor script these tests drive is a shell script, so they are
        // Unix-only - one of them Linux-only, because it reads the mode back
        // with `stat -c` rather than BSD's `stat -f`.
        //
        // Their imports follow the code. Imported unconditionally they are an
        // UNUSED IMPORT on Windows, where none of that code is compiled - and
        // `cargo clippy --all-targets -- -D warnings` says so, on a Windows
        // machine, correctly, and the temptation is to delete them. They are
        // then missing on Linux, where the code IS compiled, and the build
        // fails with "cannot find value MAX_BLOCK_BYTES" in a file nobody
        // touched.
        //
        // The lint cannot see the other platform. The import can.
        #[cfg(unix)]
        use crate::repl::{MAX_BLOCK_BYTES, MAX_BLOCK_LINES};
        #[cfg(unix)]
        use crate::value::Value;

        /// A scratch directory under the checkout, unique per test so two running
        /// in parallel cannot collide.
        fn scratch_dir(name: &str) -> PathBuf {
            static COUNTER: AtomicUsize = AtomicUsize::new(0);
            let serial = COUNTER.fetch_add(1, Ordering::SeqCst);
            let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("target/tmp/repl-onboarding")
                .join(format!("{name}-{serial}"));
            if dir.exists() {
                fs::remove_dir_all(&dir).expect("a leftover scratch directory should be removable");
            }
            fs::create_dir_all(&dir).expect("scratch directory should be creatable");
            dir
        }

        /// An editor that is a shell script writing `body` over the file it is
        /// given — the pin for `.edit`, with no real terminal editor and no TTY
        /// anywhere in the test.
        #[cfg(unix)]
        fn script_editor(dir: &Path, body: &str) -> String {
            use std::os::unix::fs::PermissionsExt;

            let path = dir.join("fake-editor.sh");
            fs::write(&path, format!("#!/bin/sh\n{body}\n"))
                .expect("the editor script is writable");
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
                .expect("the editor script is made executable");
            path.to_string_lossy().to_string()
        }

        /// What an edit left in `dir`, other than the editor script itself.
        ///
        /// The editor script is the test's own and outlives every edit it drives, so it
        /// is not a thing an edit left behind.
        fn left_behind(dir: &Path) -> Vec<String> {
            fs::read_dir(dir)
                .unwrap_or_else(|error| {
                    panic!("{} should still be readable: {error}", dir.display())
                })
                .map(|entry| {
                    entry
                        .expect("a scratch entry should be readable")
                        .file_name()
                })
                .map(|name| name.to_string_lossy().into_owned())
                .filter(|name| name != "fake-editor.sh")
                .collect()
        }

        /// That an edit left nothing of the reader's block in `dir`, whatever
        /// shape the edit left by.
        ///
        /// The whole directory rather than one file name, because the file lives
        /// in a directory of the session's own and the directory is the thing
        /// that has to go: a name that is left behind is a name the next edit,
        /// or the next session, finds.
        fn assert_no_scratch_left(dir: &Path) {
            let left = left_behind(dir);
            assert!(
                left.is_empty(),
                "no copy of the block is left in {}: {left:?}",
                dir.display()
            );
        }

        /// The tour is a numbered set of steps, and its last step is the way to
        /// the catalogue. This is the finding: `:example` existed, so a newcomer
        /// got a wall of code and no way in.
        #[test]
        fn a_tutorial_is_a_numbered_tour_that_ends_at_examples() {
            let lines = tutorial_lines(&Style::forced(false));

            for (index, step) in TUTORIAL.iter().enumerate() {
                assert!(
                    lines
                        .iter()
                        .any(|line| line.trim_start().starts_with(&format!(
                            "{}. {}",
                            index + 1,
                            step.title
                        ))),
                    "step {} is numbered and named: {:?}",
                    index + 1,
                    lines
                );
                assert!(
                    step.body
                        .lines()
                        .all(|body| lines.iter().any(|line| line.contains(body))),
                    "every step says what it is for, {:?}",
                    step.title
                );
            }

            let numbered = format!("{}. ", TUTORIAL.len());
            let final_step = lines
                .iter()
                .position(|line| line.trim_start().starts_with(&numbered))
                .unwrap_or_else(|| panic!("the tour numbers every step it has: {lines:?}"));
            let last = lines[final_step..].join("\n");
            assert!(
                last.contains(":examples"),
                "the last step is where a newcomer is sent to the catalogue, and \
                 it says {last:?}"
            );
            assert!(
                last.contains(":hint"),
                "and the tour says how to ask for help mid-step: {last:?}"
            );
            assert!(
                TUTORIAL.len() > 1,
                "a tour of one step is a wall of code again"
            );
        }

        /// Every snippet the tour shows is a program this session can run, which
        /// is the same promise [`super::EXAMPLES`] makes and is what makes a tour
        /// followable rather than aspirational.
        #[test]
        fn every_tutorial_snippet_is_a_program_the_session_runs() {
            let mut repl = Repl::new();

            for (index, step) in TUTORIAL.iter().enumerate() {
                let outcome = repl.run_code(step.snippet);
                assert!(
                    outcome.is_ok(),
                    "step {} ({}) shows {:?}, which the session refused: {:?}",
                    index + 1,
                    step.title,
                    step.snippet,
                    outcome.err()
                );
            }
        }

        /// Colour is drawn over the numbering, never instead of it: with colour
        /// forced off the steps are still numbered and there is not one escape in
        /// the output, and with it forced on the numbering is still readable in
        /// the painted lines.
        #[test]
        fn edge_tutorial_with_colour_off_numbers_its_steps_and_emits_no_escapes() {
            let off = Style::forced(false);
            let lines = tutorial_lines(&off);

            assert!(
                lines.iter().all(|line| !line.contains('\u{1b}')),
                "with colour off the tour is plain text: {lines:?}"
            );
            assert_eq!(
                lines
                    .iter()
                    .filter(|line| line.trim_start().starts_with("1. "))
                    .count(),
                1,
                "step one is numbered once and once only: {lines:?}"
            );
            assert_eq!(
                lines
                    .iter()
                    .filter(|line| line.trim_start().starts_with("2. "))
                    .count(),
                1,
                "and so is step two: {lines:?}"
            );

            let on = Style::forced(true);
            let painted = tutorial_lines(&on);
            assert_ne!(
                painted, lines,
                "with colour on the heading is painted, so the two differ at all"
            );
            for (plain, painted) in lines.iter().zip(&painted) {
                let visible = crate::repl::style::strip_escapes(painted);
                assert_eq!(
                    &visible, plain,
                    "stripping the escapes from a painted step gives the plain one: \
                     {plain:?} vs {painted:?}"
                );
            }
        }

        /// `.hint` outside the tour used to be nothing at all, which reads as a
        /// command that does nothing rather than one with nothing to say.
        #[test]
        fn a_hint_outside_the_tour_names_the_tour() {
            let lines = hint_lines(None, &Style::forced(false));

            assert_eq!(lines.len(), 1, "one answer, not none: {lines:?}");
            assert!(
                lines[0].contains(":tutorial"),
                "it names the way to start one: {:?}",
                lines[0]
            );
            assert!(
                lines[0].contains(":examples"),
                "and the catalogue it leads to: {:?}",
                lines[0]
            );
        }

        /// `.hint` in the tour answers about the step the session is on, `:tutorial` is
        /// what puts it there, and running that step's snippet is what moves it on.
        ///
        /// The second half is the half that was missing: the step was set once and
        /// nothing ever read it again, so `:hint` answered "step 1 of 6" for a
        /// reader who had already done step 1 — and the other five steps' hints
        /// could not be reached at all.
        ///
        /// Fed a line at a time, as a reader types it: `set name to "World"` is
        /// a whole program and so is `say name`, so neither run on its own is the
        /// step and only the two of them together are.
        #[test]
        fn a_hint_in_the_tour_names_the_step_it_is_for() {
            let mut repl = Repl::new();
            assert_eq!(repl.tutorial_step, None, "a fresh session is not in a tour");

            feed(&mut repl, &[":tutorial"]);

            assert_eq!(repl.tutorial_step, Some(0), ":tutorial starts at step one");
            let hint = hint_lines(repl.tutorial_step, &Style::forced(false));
            assert!(
                hint[0].contains("1") && hint[0].contains(TUTORIAL[0].hint),
                "the hint is the step's own: {:?}",
                hint[0]
            );
            assert!(
                !hint[0].contains('\u{1b}'),
                "and with colour off it is plain text: {:?}",
                hint[0]
            );

            feed(&mut repl, &["set name to \"World\""]);
            assert_eq!(
                repl.tutorial_step,
                Some(0),
                "half a step is still on the step it belongs to"
            );

            feed(&mut repl, &["say name"]);
            assert_eq!(
                repl.tutorial_step,
                Some(1),
                "and doing step one leaves the reader on step two"
            );
            let hint = hint_lines(repl.tutorial_step, &Style::forced(false));
            assert!(
                hint[0].contains("2") && hint[0].contains(TUTORIAL[1].hint),
                "so the hint now answers about step two: {:?}",
                hint[0]
            );
        }

        /// Every step of the tour can be walked, in order, by typing it — the
        /// whole point of a step being its own snippet.
        ///
        /// Line by line, so the accumulation that recognises a snippet of several
        /// programs is exercised on a step that is one (`repeat 3 times`, its
        /// body, its `end`) as well as on one that is several.
        #[test]
        fn edge_every_step_of_the_tour_can_be_walked_by_typing_it() {
            let mut repl = Repl::new();
            feed(&mut repl, &[":tutorial"]);

            for (index, step) in TUTORIAL.iter().enumerate() {
                assert_eq!(
                    repl.tutorial_step,
                    Some(index),
                    "the reader is on step {} before typing it: {:?}",
                    index + 1,
                    step.title
                );
                let lines: Vec<&str> = step.snippet.lines().collect();
                feed(&mut repl, &lines);
            }

            assert_eq!(
                repl.tutorial_step, None,
                "and a tour walked to its end is over"
            );
        }

        /// The tour ends where it says it ends: the last step's hint is the last
        /// thing a reader is on, and once it has been taken there is no step left
        /// to be on — which is what `:hint` then says.
        #[test]
        fn edge_the_last_step_taken_is_the_end_of_the_tour() {
            let mut repl = Repl::new();
            feed(&mut repl, &[":tutorial"]);
            repl.tutorial_step = Some(TUTORIAL.len() - 1);

            let lines: Vec<&str> = TUTORIAL[TUTORIAL.len() - 1].snippet.lines().collect();
            feed(&mut repl, &lines);

            assert_eq!(
                repl.tutorial_step, None,
                "a finished tour is over, not parked on its last hint"
            );
            let hint = hint_lines(repl.tutorial_step, &Style::forced(false));
            assert!(
                hint[0].contains(":examples"),
                "and the answer a reader then gets points at the catalogue the \
                 last step names: {:?}",
                hint[0]
            );
        }

        /// Only running the step moves the tour. Code that is not the step's own
        /// snippet is a reader trying something of their own, and a step they
        /// have not taken yet is a step they still need the hint for.
        #[test]
        fn edge_only_the_step_its_own_snippet_moves_the_tour_on() {
            let mut repl = Repl::new();
            feed(&mut repl, &[":tutorial"]);

            feed(&mut repl, &["set something_else to 1"]);
            assert_eq!(
                repl.tutorial_step,
                Some(0),
                "and leaves the reader on the step they were on"
            );

            feed(&mut repl, &["set name to \"World\""]);
            assert_eq!(
                repl.tutorial_step,
                Some(0),
                "and so does a snippet begun and then given up on"
            );
            feed(&mut repl, &["say \"something else\""]);
            assert_eq!(
                repl.tutorial_step,
                Some(0),
                "even after the reader strays: the tour does not skip a step"
            );

            feed(&mut repl, &["set name to \"World\"", "say nosuchvariable"]);
            assert_eq!(
                repl.tutorial_step,
                Some(0),
                "and a step that failed to run is a step that has not been taken"
            );
        }

        /// A reset is a session that has forgotten what it was, and where in the tour
        /// it had got to is part of that.
        ///
        /// A step is taken by running its snippet and a snippet needs the
        /// bindings the steps before it bound, so a session reset half-way
        /// through the tour with the tour still on the old step is a session
        /// answering `:hint` about a program it no longer holds. Half a step is
        /// the interesting half: the accumulation that recognises a step of two
        /// programs outlived the bindings it was accumulated from.
        #[test]
        fn edge_a_reset_forgets_the_tour_it_was_walked_through() {
            let mut repl = Repl::new();
            feed(&mut repl, &[":tutorial"]);
            feed(&mut repl, &["set name to \"World\""]);

            assert_eq!(repl.tutorial_step, Some(0), "mid-step, before the reset");
            assert!(
                !repl.tutorial_typed.is_empty(),
                "with half of the step's snippet accumulated: {:?}",
                repl.tutorial_typed
            );

            feed(&mut repl, &[":reset"]);

            assert_eq!(
                repl.tutorial_step, None,
                "a reset session is not part-way through a tour"
            );
            assert!(
                repl.tutorial_typed.is_empty(),
                "and the half of the snippet it had is gone with it: {:?}",
                repl.tutorial_typed
            );
            let hint = hint_lines(repl.tutorial_step, &Style::forced(false));
            assert!(
                hint[0].contains(":tutorial"),
                "so the hint asks for a tour rather than answering about a step \
                 whose bindings are gone: {:?}",
                hint[0]
            );

            // Which is what makes the next tour start clean rather than
            // half-way through the one before it.
            feed(&mut repl, &[":tutorial"]);
            assert_eq!(
                repl.tutorial_step,
                Some(0),
                "and a new tour starts at step one"
            );
            assert!(
                repl.tutorial_typed.is_empty(),
                "with nothing of the last one still half-typed: {:?}",
                repl.tutorial_typed
            );
        }

        /// The round trip: the block the session is holding goes out to the
        /// editor, and what the editor wrote comes back as a program that ran.
        ///
        /// Driven through [`Repl::edit_with`], the entry point the command uses,
        /// so what is asserted is the session's own state afterwards and not what
        /// a scratch file happens to hold.
        #[cfg(unix)]
        #[test]
        fn edit_runs_what_the_editor_left_behind() {
            let dir = scratch_dir("edit-round-trip");
            let editor = script_editor(&dir, "printf 'set edited to 42\n' > \"$1\"");
            let mut repl = Repl::new();
            feed(&mut repl, &["repeat 2 times"]);

            assert!(repl.in_multiline, "the block is open before the edit");

            repl.edit_with(Some(&editor), &dir)
                .expect("an editor that saves the file is an editor that worked");

            assert!(
                repl.vm.user_names().contains(&"edited".to_string()),
                "what the editor wrote is a program the session ran: {:?}",
                repl.vm.user_names()
            );
            assert!(
                repl.multiline_buffer.is_empty() && !repl.in_multiline,
                "and a block that ran is not left to be run again by the next line"
            );
            assert_no_scratch_left(&dir);
        }

        /// What the reader was writing is theirs: while the editor is open, the
        /// file it was handed is readable by its owner and nobody else, and it
        /// sits in a directory of its own rather than in a world-writable one
        /// where a name this session would pick could be planted beforehand.
        ///
        /// Linux, not `cfg(unix)`: the editor reports the modes with
        /// `stat -c '%a'`, which is GNU. BSD and macOS spell that `-f '%Lp'`, so
        /// on those hosts the script would write two empty strings and the block
        /// would not be the program this asserts on. The property is pinned where
        /// the command that reads it back exists.
        #[cfg(target_os = "linux")]
        #[test]
        fn edge_the_scratch_file_is_readable_by_its_owner_and_nobody_else() {
            let dir = scratch_dir("edit-private-scratch");
            // The editor reports back what it was given: the modes of the file it
            // opened and of the directory it is in, as a program that runs.
            let editor = script_editor(
                &dir,
                "printf 'set file_mode to %s\\nset dir_mode to %s\\n' \
                 \"$(stat -c '%a' \"$1\")\" \"$(stat -c '%a' \"$(dirname \"$1\")\")\" > \"$1\"",
            );
            let mut repl = Repl::new();
            feed(&mut repl, &["repeat 2 times"]);

            repl.edit_with(Some(&editor), &dir)
                .expect("an editor that only reports the modes still edits");

            assert_eq!(
                repl.lookup("file_mode"),
                Some(&Value::Number(600.0)),
                "the file holding the block is not readable by anyone else"
            );
            assert_eq!(
                repl.lookup("dir_mode"),
                Some(&Value::Number(700.0)),
                "and it is not in a directory another user can list"
            );
            assert_no_scratch_left(&dir);
        }

        /// The scratch file is created, and never opened at a name somebody else
        /// could have got there first: a name that is taken is a name the session
        /// does not write into.
        ///
        /// The name the session would pick is `redblue-edit-<pid>-<serial>`, so
        /// that is the name planted here — planting the pid alone would test a
        /// path nothing ever opens.
        #[cfg(unix)]
        #[test]
        fn edge_a_scratch_path_already_taken_is_refused_rather_than_followed() {
            let dir = scratch_dir("edit-symlink-scratch");
            let planted = dir.join(format!("redblue-edit-{}-0", std::process::id()));
            fs::create_dir(&planted).expect("the planted directory should be creatable");
            fs::write(planted.join("decoy.rb"), "set planted to 1\n")
                .expect("the planted directory should be writable");

            let mut repl = Repl::new();
            feed(&mut repl, &["repeat 2 times"]);
            let editor = script_editor(&dir, "printf 'set edited to 1\\n' > \"$1\"");

            // The name is taken, so the session takes the next serial rather than
            // writing into a directory it did not make.
            repl.edit_with(Some(&editor), &dir)
                .expect("the next serial is a directory of its own, so the edit works");
            assert!(
                !repl.lookup("planted").is_some(),
                "and nothing of the session's ran out of the planted directory"
            );
            assert!(
                repl.vm.user_names().contains(&"edited".to_string()),
                "while the editor's own block ran: {:?}",
                repl.vm.user_names()
            );
            assert_eq!(
                left_behind(&dir),
                vec![format!("redblue-edit-{}-0", std::process::id())],
                "and the serial the session made is gone while the planted one is \
                 not its to remove"
            );
            assert_eq!(
                fs::read_dir(&planted)
                    .expect("the planted directory should still be readable")
                    .count(),
                1,
                "with nothing of the session's written inside it"
            );
        }

        /// A name is not reused, it is skipped — and when every name this session
        /// would try is taken, it says which range it tried rather than reporting
        /// a generic failure.
        ///
        /// `/tmp` is shared, so "the name is free" is not a thing a session can
        /// assume; the honest answer is to search, and to report the search when
        /// it runs out. Sixty-four serials of `redblue-edit-<pid>-<serial>`.
        #[test]
        fn edge_every_scratch_name_taken_is_reported_as_the_range_it_tried() {
            let dir = scratch_dir("edit-no-free-serial");
            for serial in 0..SCRATCH_DIR_ATTEMPTS {
                fs::create_dir(dir.join(format!("redblue-edit-{}-{serial}", std::process::id())))
                    .expect("a planted directory should be creatable");
            }
            let repl = Repl::new();

            let error = repl
                .private_scratch_dir(&dir)
                .expect_err("no free name is not a directory of its own");

            let last = SCRATCH_DIR_ATTEMPTS - 1;
            assert!(
                error.contains(&format!("redblue-edit-{}-0", std::process::id()))
                    && error.contains(&format!("redblue-edit-{}-{last}", std::process::id())),
                "and the message names both ends of the search: {error}"
            );
            assert_eq!(
                left_behind(&dir).len(),
                SCRATCH_DIR_ATTEMPTS,
                "and the directories it found taken are the ones the test planted: \
                 none of them was written into"
            );
        }

        /// No editor named is a clean error that says which variable to set, and
        /// the block the session was holding survives it.
        #[test]
        fn edge_edit_without_an_editor_names_the_variable_and_keeps_the_buffer() {
            let dir = scratch_dir("edit-no-editor");
            let mut repl = Repl::new();
            feed(&mut repl, &["repeat 2 times", "    say \"hi\""]);

            let error = repl
                .edit_with(None, &dir)
                .expect_err("no editor named is not an edit");

            assert!(
                error.contains("EDITOR"),
                "the message names the variable the reader has to set: {error}"
            );
            assert!(
                repl.in_multiline,
                "and the session is still holding the block it was holding"
            );
            assert_eq!(
                repl.multiline_buffer,
                vec!["repeat 2 times".to_string(), "    say \"hi\"".to_string()],
                "the buffer is preserved exactly, not truncated or cleared"
            );
        }

        /// An editor that cannot be run, one that fails, and one that empties the
        /// file are three different failures and none of them runs anything.
        ///
        /// Driven through [`Repl::edit_with`] rather than through the file round
        /// trip beneath it: the buffer is this session's own state, so a claim
        /// about it means nothing over a `&self` that cannot have touched it.
        #[test]
        fn edge_an_editor_that_cannot_be_used_is_reported_rather_than_run() {
            let dir = scratch_dir("edit-bad-editor");

            let mut repl = Repl::new();
            feed(&mut repl, &["repeat 2 times"]);

            let error = repl
                .edit_with(Some(&dir.join("no-such-editor").to_string_lossy()), &dir)
                .expect_err("a program that is not there cannot edit anything");
            assert!(
                error.contains("no-such-editor"),
                "and it says which program failed: {error}"
            );
            assert_eq!(
                repl.multiline_buffer,
                vec!["repeat 2 times".to_string()],
                "nothing was run and nothing was lost"
            );

            #[cfg(unix)]
            {
                let failing = script_editor(&dir, "exit 3");
                let error = repl
                    .edit_with(Some(&failing), &dir)
                    .expect_err("an editor that exits badly has not saved anything");
                assert!(
                    error.contains("exited"),
                    "a failing editor is reported as having failed: {error}"
                );

                let emptying = script_editor(&dir, ": > \"$1\"");
                let error = repl
                    .edit_with(Some(&emptying), &dir)
                    .expect_err("an empty file is not a program");
                assert!(
                    error.contains("nothing to run"),
                    "and an emptied file says so rather than running nothing: {error}"
                );
                assert_eq!(
                    repl.multiline_buffer,
                    vec!["repeat 2 times".to_string()],
                    "every one of them left the block the session was holding"
                );
            }
        }

        /// A block the editor wrote that does not run is the only copy of itself
        /// there is, and clearing the buffer before the run threw it away with no
        /// way back into it.
        ///
        /// `.edit` is the only route into the session for anything longer than a
        /// line, so a typo at the end of a program the reader had just written was
        /// a program lost. It comes back into the buffer here, and the buffer is
        /// the state a test can hold.
        #[cfg(unix)]
        #[test]
        fn edge_an_edit_that_does_not_run_gives_the_block_back() {
            let dir = scratch_dir("edit-broken-program");
            let editor = script_editor(&dir, "printf 'set kept to 1\\nrepeat 2 times\\n' > \"$1\"");
            let mut repl = Repl::new();
            feed(&mut repl, &["repeat 2 times"]);

            let error = repl
                .edit_with(Some(&editor), &dir)
                .expect_err("a block that never balances is not a program");

            assert!(
                error.contains("buffer"),
                "and the message says where the block went: {error}"
            );
            assert_eq!(
                repl.multiline_buffer,
                vec!["set kept to 1".to_string(), "repeat 2 times".to_string()],
                "the block the editor saved is back in the buffer, in full"
            );
            assert!(
                repl.in_multiline,
                "and it is still an open block, so `end` finishes it rather than \
                 than being appended to something else"
            );

            // Which is the point of putting it back: the block is editable again.
            feed(&mut repl, &["end"]);
            assert!(
                repl.vm.user_names().contains(&"kept".to_string()),
                "and finishing it by hand is a program the session runs: {:?}",
                repl.vm.user_names()
            );
        }

        /// The scratch file is the session's own buffer, written out so an editor
        /// has something to open — and the edit paths that failed left it there.
        ///
        /// The success path removed it and nothing else did, so an editor that
        /// could not be run, or that exited badly, or whose save could not be
        /// read back, each left a copy of what the reader was writing in the
        /// temp directory. Three failed edits is three copies.
        #[cfg(unix)]
        #[test]
        fn edge_a_failed_edit_leaves_no_copy_of_the_buffer_behind() {
            let dir = scratch_dir("edit-no-scratch-left");
            let mut repl = Repl::new();
            feed(&mut repl, &["repeat 2 times"]);

            let missing = dir.join("no-such-editor").to_string_lossy().into_owned();
            assert!(
                repl.edit_with(Some(&missing), &dir).is_err(),
                "an editor that is not there cannot edit anything"
            );
            assert_no_scratch_left(&dir);

            let failing = script_editor(&dir, "exit 3");
            assert!(
                repl.edit_with(Some(&failing), &dir).is_err(),
                "an editor that exits badly has saved nothing"
            );
            assert_no_scratch_left(&dir);
        }

        /// The directory the editor was opened over is taken with it however the
        /// edit ended — including on the paths that end before the editor runs.
        ///
        /// The removal was once written at the end of the successful path and at
        /// the end of the failing one, and the two paths between them — a lock
        /// that failed, a buffer that could not be written — returned before
        /// either, so they left the directory in the temp directory holding the
        /// block. The removal is now in `Drop`, which every path out of the edit
        /// goes through whatever its shape.
        ///
        /// A real `chmod` or write failure cannot be forced without a second user
        /// or a full disk, so the guard is driven where it can be: an owned
        /// directory, given up on part-way through the work, is gone. The second
        /// half is a genuine I/O failure, reached end to end.
        #[test]
        fn edge_a_scratch_directory_is_taken_with_it_however_the_edit_ended() {
            let dir = scratch_dir("edit-scratch-dir-guard");
            let repl = Repl::new();

            let owned = repl
                .private_scratch_dir(&dir)
                .expect("a free serial makes a directory of its own");
            let block = owned.path().join("redblue-edit.rb");
            repl.write_scratch_file(&block)
                .expect("the block should be writable inside a directory of its own");
            drop(owned);
            assert!(
                !block.exists(),
                "giving the directory up part-way through is how `?` leaves it"
            );
            assert_no_scratch_left(&dir);

            #[cfg(unix)]
            {
                // A real failure inside the edit: the editor saves a directory
                // where the file was, so the read back cannot answer.
                let mut repl = Repl::new();
                feed(&mut repl, &["repeat 2 times"]);
                let replacing = script_editor(&dir, "rm -f \"$1\" && mkdir \"$1\"");
                let error = repl
                    .edit_with(Some(&replacing), &dir)
                    .expect_err("a directory where the save was is not a program");
                assert!(
                    error.contains("could not read back"),
                    "and it is the read back that is reported: {error}"
                );
                assert_no_scratch_left(&dir);
                assert_eq!(
                    repl.multiline_buffer,
                    vec!["repeat 2 times".to_string()],
                    "with the block the session was holding still in the buffer"
                );
            }
        }

        /// A block past what a session holds is a block the session refuses to
        /// read, at the line limit and at the byte limit, and a block exactly at
        /// either one is still a block it runs.
        ///
        /// `.edit` is the other way into the session and the limit was not on
        /// the keyboard: whatever an editor wrote was read whole and parsed, so
        /// a runaway `>` could hand the session a block that no typed line could
        /// have made. Both sides of both limits, because a limit drawn one byte
        /// or one line too early is a limit that refuses a block it should run.
        #[cfg(unix)]
        #[test]
        fn edge_a_block_the_editor_saved_past_the_session_limit_is_refused() {
            let dir = scratch_dir("edit-too-large");
            let lines = |count: usize| -> String {
                format!(
                    "i=0; while [ $i -lt {count} ]; do echo \"set v to $i\"; \
                     i=$((i+1)); done > \"$1\""
                )
            };
            // `set big to "` and the closing quote and the newline, so a payload
            // of this many bytes is a file of exactly the limit.
            let overhead = "set big to \"".len() + 2;
            let payload = |bytes: usize| -> String {
                format!(
                    "printf 'set big to \"%s\"\\n' \
                     \"$(head -c {bytes} /dev/zero | tr '\\000' 'x')\" > \"$1\""
                )
            };

            let mut repl = Repl::new();

            let at_line_limit = script_editor(&dir, &lines(MAX_BLOCK_LINES));
            feed(&mut repl, &["repeat 2 times"]);
            repl.edit_with(Some(&at_line_limit), &dir)
                .expect("a block exactly at the line limit is one the session holds");
            assert_eq!(
                repl.lookup("v"),
                Some(&Value::Number((MAX_BLOCK_LINES - 1) as f64)),
                "and it ran: the last of its lines was the one that set it"
            );

            let over_line_limit = script_editor(&dir, &lines(MAX_BLOCK_LINES + 1));
            feed(&mut repl, &["repeat 2 times"]);
            let error = repl
                .edit_with(Some(&over_line_limit), &dir)
                .expect_err("one line past the limit is past it");
            assert!(
                error.contains("nothing was read back from it"),
                "and it says the block was not read rather than that it did not run: \
                 {error}"
            );
            assert_eq!(
                repl.multiline_buffer,
                vec!["repeat 2 times".to_string()],
                "with the block the session was holding untouched"
            );

            let at_byte_limit = script_editor(&dir, &payload(MAX_BLOCK_BYTES - overhead));
            feed(&mut repl, &["repeat 2 times"]);
            repl.edit_with(Some(&at_byte_limit), &dir)
                .expect("a block exactly at the byte limit is one the session holds");
            assert!(
                repl.lookup("big").is_some(),
                "and it ran: the string it set is bound"
            );

            let over_byte_limit = script_editor(&dir, &payload(MAX_BLOCK_BYTES - overhead + 1));
            feed(&mut repl, &["repeat 2 times"]);
            let error = repl
                .edit_with(Some(&over_byte_limit), &dir)
                .expect_err("one byte past the limit is past it");
            assert!(
                error.contains("nothing was read back from it"),
                "and it is refused before it is read: {error}"
            );
            assert_eq!(
                repl.multiline_buffer,
                vec!["repeat 2 times".to_string()],
                "with the block the session was holding untouched"
            );
            assert_no_scratch_left(&dir);
        }

        /// A save that is not text is not a program, and saying so is not the same as
        /// saying it failed to read.
        ///
        /// The read is bounded by the session's block limit, but the bytes inside
        /// that bound are still decoded as UTF-8, so an editor that wrote a
        /// binary file — a stray `dd`, an editor handed the wrong path — is
        /// reported as what it is rather than as an I/O failure the reader cannot
        /// act on.
        #[cfg(unix)]
        #[test]
        fn edge_a_save_that_is_not_text_is_refused_rather_than_parsed() {
            let dir = scratch_dir("edit-not-text");
            let binary = script_editor(&dir, "printf 'set x to \"\\377\"' > \"$1\"");
            let mut repl = Repl::new();
            feed(&mut repl, &["repeat 2 times"]);

            let error = repl
                .edit_with(Some(&binary), &dir)
                .expect_err("a byte that is not text is not a program");

            assert!(
                error.contains("is not text"),
                "and the message says what was saved rather than blaming the read: \
                 {error}"
            );
            assert_eq!(
                repl.multiline_buffer,
                vec!["repeat 2 times".to_string()],
                "with the block the session was holding untouched"
            );
            assert_no_scratch_left(&dir);
        }

        /// An `$EDITOR` that names nothing at all is a different mistake from one
        /// that names a program that is not there, and it is refused by name
        /// rather than spawned.
        ///
        /// `editor_program` cannot hand this over — it filters the empty and the
        /// blank away before `.edit` sees them — so `edit_in` is driven directly
        /// with what `$EDITOR` can actually hold. Nothing is written either way:
        /// the name is refused before there is a directory to write it into.
        #[test]
        fn edge_an_editor_that_names_nothing_is_refused_rather_than_spawned() {
            let dir = scratch_dir("edit-blank-editor");
            let repl = Repl::new();

            for blank in ["", "   ", "\t "] {
                let error = repl
                    .edit_in(Some(blank), &dir)
                    .expect_err("a name with no program in it is not an editor");
                assert!(
                    error.contains("is not a program that can be run"),
                    "and the message says the name is not a program: {error}"
                );
            }
            assert_no_scratch_left(&dir);
        }

        /// The commands are commands: they parse, they are completable, and
        /// `:help` names them.
        ///
        /// `.edit` is checked by parsing rather than by dispatching, on purpose:
        /// dispatching it would read the *process's* `EDITOR` and open whatever
        /// that is, which is a terminal editor in a machine that has one.
        #[test]
        fn edge_the_onboarding_commands_are_reachable_every_way() {
            use crate::repl::commands::ReplCommand;

            let mut repl = Repl::new();
            for command in [":tutorial", ".tutorial", ":tour", ":hint", ".hint"] {
                assert!(
                    repl.complete(command).contains(&command.to_string()),
                    "{command} is a command, so TAB offers it: {:?}",
                    repl.complete(command)
                );
                feed(&mut repl, &[command]);
            }

            assert_eq!(
                ReplCommand::parse(".edit"),
                ReplCommand::Edit,
                "and `.edit` parses as the command that opens an editor"
            );
            assert_eq!(
                ReplCommand::parse(":edit"),
                ReplCommand::Edit,
                "in either sigil, like every other command"
            );

            let help = ReplCommand::help_lines();
            assert!(
                help.iter().any(|line| line.contains(":tutorial"))
                    && help.iter().any(|line| line.contains(":hint"))
                    && help.iter().any(|line| line.contains(":edit")),
                "and :help names all three: {help:?}"
            );
        }

        /// A hint is painted whole, so the reader can tell it from the source and
        /// the text itself survives the painting.
        ///
        /// Both of `.hint`'s answers, because they used to disagree: the "no tour"
        /// one was painted and the one a reader actually gets mid-tour was not,
        /// so the single answer `:hint` gives in a tour was the only one of the
        /// two that looked like everything else the session prints.
        #[test]
        fn edge_a_painted_hint_keeps_the_hint_readable() {
            let on = Style::forced(true);
            let off = Style::forced(false);

            for step in [None, Some(0)] {
                let painted = hint_lines(step, &on);

                assert!(
                    painted[0].starts_with("\u{1b}"),
                    "with colour on the hint is painted, so it begins with an \
                     escape: {:?}",
                    painted[0].escape_debug()
                );
                assert_eq!(
                    painted[0],
                    on.paint(Paint::Heading, hint_lines(step, &off)[0].as_str()),
                    "the painted hint is the plain hint painted, and nothing else \
                     (step {step:?})"
                );
                assert_eq!(
                    crate::repl::style::strip_escapes(&painted[0]),
                    hint_lines(step, &off)[0],
                    "and what it says survives the painting (step {step:?}): {:?}",
                    painted[0].escape_debug()
                );
            }
        }
    }
}
