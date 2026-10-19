//! The debug sub-REPL: a session inside the session, entered with
//! `.debug <expression|file>` and left with `.back`.
//!
//! Everything it observes — its VM, its watches, its stack trace — is a copy of
//! the parent session's state, so nothing the sub-REPL runs can bind a name in
//! the parent, write a transcript line or touch the parent's history. The
//! parent's own state is what `.back` returns to, byte for byte as it was left.
//!
//! Observation happens through the VM's debug hook: a statement-boundary event
//! fires before every statement, the event's frame information is copied into a
//! trace the sub-REPL reads, and every registered `.watch` expression is
//! re-evaluated against that step's bindings on a scratch VM the hook owns. The
//! hook never changes what the program does — a watch that fails is a line of
//! report, not an error in the run.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::analyzer;
use crate::debugger::{DebugAction, DebugEvent};
use crate::interpreter::Vm;
use crate::parser::Program;
use crate::repl::commands::ReplCommand;
use crate::repl::style::{Paint, Style};
use crate::repl::{failure_line, parse_only, render_error, Repl};
use crate::value::{CapturedScope, Value};

use super::introspect::SESSION_FILE;
use super::BLOCK_REPORT_LINES;

/// What the session is offered instead of a `.debug` that would enter a
/// sub-REPL: the word for opening one, and nothing else.
pub(super) const NO_DEBUG_SESSION: &str =
    "No debug session is open. Type .debug <expression|file> to open one.";

/// The one line printed when a session that is already inside the sub-REPL is
/// asked to enter it again with a bare `.debug`.
const ALREADY_IN_SESSION: &str = "Already in a debug session; .back returns to the session.";

/// The whole of `:help` inside the sub-REPL: six commands, not the session's
/// table, because the session's commands cannot be answered here.
const SUB_HELP: &str = "Debug sub-REPL commands: .watch <expression> reports a value \
                        when it changes, .stack shows the call stack, .locals shows \
                        the frame last stepped in, .run <file> runs a file here, \
                        .debug <expression|file> runs more code here, .back returns \
                        to the session.";

/// What a session command that belongs to the parent prompt is answered with
/// inside the sub-REPL.
const SUB_ONLY_MESSAGE: &str = "Only .watch, .stack, .locals, .run, .debug and .back are \
                                available in the debug sub-REPL; .back returns to \
                                the session.";

/// The line printed when a session opens, above whatever was asked to run on
/// entry.
pub(super) const BANNER: &str = "Debug sub-REPL: .watch <expression> reports changes, .stack \
                      shows the call stack, .locals shows the current frame, .run <file> \
                      runs a file here, .back returns to the session.";

/// One registered watch: what the user asked to watch, the program it parses
/// to, and the value it last had — or `None` until it has had one.
struct Watch {
    source: String,
    program: Program,
    last: Option<Value>,
}

/// The step information `.stack` and `.locals` read, as of the last statement
/// the hook saw.
#[derive(Default)]
struct Trace {
    /// Whether any statement has been observed at all — what tells "nothing has
    /// run yet" apart from "a program ran and was at the top level".
    observed: bool,
    stack: Vec<String>,
    locals: Vec<(String, Value)>,
    /// Watch reports generated during the run that has not been read yet.
    reports: Vec<String>,
}

/// The state the hook writes and the commands read, shared between them
/// because the hook lives inside the VM it fires from.
struct State {
    watches: Vec<Watch>,
    trace: Trace,
    /// The watcher's own VM, held here rather than in the hook so the run
    /// itself can evaluate the watches once more against the bindings the
    /// program *finished* with: a value changed by the program's last
    /// statement is first visible at the end, not at any statement stop.
    scratch: Vm,
}

/// A debug session: a fork of the parent's VM with a hook attached, the
/// watches registered against it, and the block the sub-REPL is buffering.
pub struct DebugRepl {
    vm: Vm,
    state: Arc<Mutex<State>>,
    buffer: Vec<String>,
    style: Style,
    /// The sub-REPL's own `_`, kept here so the parent's last result is never
    /// written to by anything the sub-REPL runs.
    variables: HashMap<String, Value>,
}

impl DebugRepl {
    /// A sub-REPL over `vm` — a [`Vm::fork`] of the parent's — with the hook
    /// that observes every statement it runs already attached.
    pub(super) fn new(mut vm: Vm, style: Style) -> Self {
        // The watcher's own VM: bindings are overwritten at every step (see
        // [`Vm::observe_bindings`]), and it is told it is answering a question
        // so an effectful builtin — `files.write`, `network.post` — is refused
        // at the call rather than run for real on behalf of an expression that
        // was only being watched.
        let mut scratch = Vm::new();
        scratch.ask_questions(true);
        let state = Arc::new(Mutex::new(State {
            watches: Vec::new(),
            trace: Trace::default(),
            scratch,
        }));
        let hook_state = Arc::clone(&state);
        vm.set_debug_hook(Some(Box::new(move |event| {
            let mut state = hook_state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            observe(&mut state, &event);
            DebugAction::Run
        })));
        Self {
            vm,
            state,
            buffer: Vec::new(),
            style,
            variables: HashMap::new(),
        }
    }

    /// The names this sub-REPL's code may be analyzed against: its own `_` and
    /// everything the forked VM holds, which is everything the parent session
    /// held when the sub-REPL was entered plus whatever it has run since.
    pub(super) fn session_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.variables.keys().cloned().collect();
        names.extend(self.vm.user_names());
        names.sort();
        names.dedup();
        names
    }

    /// The prompt the sub-REPL reads at, numbered while a block is open exactly
    /// as the parent's is — a block is typed here the same way it is typed
    /// there, and the numbering is what says which line of it is being typed.
    pub(super) fn prompt(&self) -> String {
        if self.buffer.is_empty() {
            "debug> ".to_string()
        } else {
            format!("..  {} ", self.buffer.len() + 1)
        }
    }

    /// Registers `source` as a watch, or reports why it cannot be one.
    ///
    /// The expression is not evaluated here. A watch begins with no previous
    /// value and takes the first one it sees as its baseline — so a watch set
    /// on a name that does not exist yet says nothing until the run that binds
    /// it, and every later step where the value differs from the last one it
    /// had is a line of report. What *cannot* be watched is something that does
    /// not parse: no run would ever make it an expression.
    ///
    /// A watch asked for with no expression at all is a question about the
    /// watches this session already holds, and is answered with them.
    pub(super) fn watch(&mut self, source: &str) -> Vec<String> {
        if source.trim().is_empty() {
            return self.watch_lines();
        }
        let program = match parse_only(source) {
            Ok(program) => program,
            Err(error) => return vec![failure_line(&self.style, source, &error)],
        };
        if let Ok(mut state) = self.state.lock() {
            state.watches.push(Watch {
                source: source.to_string(),
                program,
                last: None,
            });
        }
        vec![format!("Watching {}.", source)]
    }

    /// The watches this session holds, one per line.
    ///
    /// A session that holds none says so: "no watches" is an answer, and a
    /// session that printed nothing at all would be read as a command that did
    /// not work.
    pub(super) fn watch_lines(&self) -> Vec<String> {
        let state = lock(&self.state);
        if state.watches.is_empty() {
            return vec!["No watches.".to_string()];
        }
        let mut lines = vec![format!("Watching {} expression(s):", state.watches.len())];
        for watch in &state.watches {
            lines.push(format!("  {}", watch.source));
        }
        lines
    }

    /// The call stack of the last step taken, outermost frame first.
    ///
    /// Before anything has run there is nothing to name — including right after
    /// a bare `.debug`, which opens the sub-REPL without running anything.
    pub(super) fn stack_lines(&self) -> Vec<String> {
        let state = lock(&self.state);
        if !state.trace.observed || state.trace.stack.is_empty() {
            return vec!["No live frames.".to_string()];
        }
        let count = state.trace.stack.len();
        let mut lines = vec![format!(
            "Call stack ({} frame{}):",
            count,
            if count == 1 { "" } else { "s" }
        )];
        for name in &state.trace.stack {
            lines.push(format!("  {}", name));
        }
        lines
    }

    /// The bindings of the frame the last step ran in: its parameters, its
    /// locals, and the globals it wrote itself — never its caller's.
    pub(super) fn locals_lines(&self) -> Vec<String> {
        let state = lock(&self.state);
        if !state.trace.observed {
            return vec!["No live frames.".to_string()];
        }
        if state.trace.locals.is_empty() {
            return vec!["Locals: (no bindings)".to_string()];
        }
        let mut lines = vec!["Locals:".to_string()];
        for (name, value) in &state.trace.locals {
            lines.push(format!("  {} = {}", name, value));
        }
        lines
    }

    /// Buffers one line of Redblue and runs the block when it balances, the
    /// way the parent session's own line dispatch does — with its own buffer,
    /// so a half-typed block here is not a half-typed block there.
    pub(super) fn push_line(&mut self, line: &str) -> Vec<String> {
        self.buffer.push(line.to_string());
        let buffered = self.buffer.join("\n");
        if Repl::exceeds_block_limits(self.buffer.len(), buffered.len()) {
            return self.report_block_too_large();
        }
        if Repl::needs_continuation(&buffered) {
            return Vec::new();
        }
        let code = std::mem::take(&mut self.buffer).join("\n");
        self.run(&code, SESSION_FILE)
    }

    /// Reports a block the sub-REPL was holding when the input ended, the way
    /// [`Repl::report_unclosed_block`] reports the parent's.
    pub(super) fn report_unclosed_block(&mut self) -> Vec<String> {
        if self.buffer.is_empty() {
            return Vec::new();
        }
        let code = std::mem::take(&mut self.buffer).join("\n");
        let lines: Vec<&str> = code.lines().collect();
        let mut answers = vec![format!(
            "Error: incomplete block: {} line(s) were never closed with 'end'.",
            lines.len()
        )];
        for line in lines.iter().take(BLOCK_REPORT_LINES) {
            answers.push(format!("  {}", line));
        }
        if lines.len() > BLOCK_REPORT_LINES {
            answers.push(format!(
                "  ... and {} more line(s)",
                lines.len() - BLOCK_REPORT_LINES
            ));
        }
        answers
    }

    /// Gives up on a block that outgrew what the sub-REPL will hold, saying so
    /// rather than buffering on — the parent's report, of the parent's limits.
    fn report_block_too_large(&mut self) -> Vec<String> {
        let lines = self.buffer.len();
        let bytes: usize = self.buffer.iter().map(|line| line.len() + 1).sum();
        self.buffer.clear();
        vec![format!(
            "Error: incomplete block: dropping a block of {} line(s) and {} byte(s) \
             that never balanced; a session holds at most {} lines or {} bytes.",
            lines,
            bytes,
            super::MAX_BLOCK_LINES,
            super::MAX_BLOCK_BYTES
        )]
    }

    /// Runs `code` against this sub-REPL's bindings and answers with everything
    /// the run produced: the lines it printed, its result or its failure, and
    /// any watch reports the run generated.
    ///
    /// Analysed against the sub-REPL's own names, exactly as the parent
    /// analyses a typed line against the parent's — so `set x to 1` here can be
    /// read by the next line here, and neither is visible on the other side of
    /// `.back`.
    pub(super) fn run(&mut self, code: &str, file: &str) -> Vec<String> {
        let program = match parse_only(code) {
            Ok(program) => program,
            Err(error) => return vec![failure_line(&self.style, code, &error)],
        };
        let names = self.session_names();
        if let Err(error) = analyzer::analyze_with_bound_names(&program, &names) {
            return vec![failure_line(&self.style, code, &error)];
        }

        self.vm.take_output();
        let result = self.vm.run_quietly(&program);
        let mut lines = self.vm.take_output();
        // One more watch evaluation, against the bindings the run ended on: a
        // value changed by the program's *last* statement has no statement
        // stop after it to be seen at, and the end of the run is that stop.
        {
            let mut state = lock(&self.state);
            let (scopes, globals) = self.vm.current_bindings();
            evaluate_watches(&mut state, scopes, globals);
        }
        match result {
            Ok(value) => {
                if !matches!(value, Value::Nothing) {
                    let line = self.style.paint(Paint::Result, &format!("= {}", value));
                    self.variables.insert("_".to_string(), value);
                    lines.push(line);
                }
            }
            Err(error) => {
                lines.push(format!(
                    "Error: {}",
                    render_error(&self.style, code, file, &error)
                ));
            }
        }
        lines.extend(self.take_reports());
        lines
    }

    /// Runs the file `path` names, in this sub-REPL, under observation.
    ///
    /// This is what makes a *file* watchable: a file run at `.debug` time is
    /// run before there is a watch to see it, and the whole point of the
    /// sub-REPL is watching something. So the file is run here instead, after
    /// the watches are set, and the same report lines come out of it.
    ///
    /// A file that cannot be read is one error line: the run failed, and the
    /// session that asked for it is still a session.
    pub(super) fn run_file(&mut self, path: &str) -> Vec<String> {
        match read_source(path) {
            Ok(source) => self.run(&source, path),
            Err(message) => vec![format!("Error: {}", message)],
        }
    }

    /// The watch reports the runs so far have generated, cleared: a report is
    /// printed once, on the answer to the line whose run produced it.
    fn take_reports(&self) -> Vec<String> {
        self.state
            .lock()
            .map(|mut state| std::mem::take(&mut state.trace.reports))
            .unwrap_or_default()
    }
}

/// The source of the file `path` names, or the failure that says why it could
/// not be read.
///
/// A `Result` rather than a printed line, so the caller decides what a failure
/// looks like: the sub-REPL reports it and carries on, and a test that watches
/// a file which is not there asserts the failure itself rather than the
/// wording of a message.
pub(super) fn read_source(path: &str) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|error| format!("cannot read '{}': {}", path, error))
}

/// Locks the shared state, ignoring a poisoned lock — the same contract
/// [`crate::debugger::lock`] keeps: a watcher that panicked is one failed
/// watch, not a sub-REPL that can never be read again.
fn lock(state: &Arc<Mutex<State>>) -> MutexGuard<'_, State> {
    state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Records one statement-boundary event and evaluates every watch against it.
///
/// The trace is overwritten rather than accumulated: `.stack` and `.locals`
/// answer about the *last* step, and a watch reports only when its value
/// differs from the one it last saw — which is a step count the event itself
/// carries nothing about, so a change is a change whether five statements ran
/// in between or none.
fn observe(state: &mut State, event: &DebugEvent<'_>) {
    state.trace.observed = true;
    state.trace.stack = event.call_stack.to_vec();
    state.trace.locals = event.frame_locals();
    evaluate_watches(state, event.scopes, event.globals);
}

/// Evaluates every registered watch against `scopes`/`globals` and appends a
/// line of report for each one whose value moved.
///
/// This is the whole watch contract, in one place — the statement stops call it
/// with the step's bindings, and a finished run calls it once more with the
/// bindings the program ended on, so a value the last statement changed is
/// still reported. Two things are deliberately *not* reported:
///
/// - **The first value a watch ever sees.** A watch set on a name with a value
///   has not seen a change yet — only the value it was set on. Its baseline is
///   taken silently, and every later difference from it is a line.
/// - **A change the program makes after the last stop.** There is no such
///   thing at a stop, but there is at the end of the run, which is why the
///   final pass exists; without it `watch x = 3` for a loop that ended at 3
///   would never print, because no statement runs after the one that produced
///   3 to observe it with.
///
/// A watch that fails reports one line per evaluation and keeps its previous
/// value: the failure is about this step, not about the value it last held.
fn evaluate_watches(state: &mut State, scopes: &[CapturedScope], globals: &HashMap<String, Value>) {
    let State {
        watches,
        trace,
        scratch,
    } = state;
    for watch in watches.iter_mut() {
        scratch.observe_bindings(scopes, globals);
        match scratch.run_quietly(&watch.program) {
            Ok(value) => {
                let previous = watch.last.replace(value.clone());
                if matches!(previous, Some(previous) if previous != value) {
                    trace
                        .reports
                        .push(format!("watch {} = {}", watch.source, value));
                }
            }
            Err(error) => {
                // One line, for one step: the rendered failure would draw a
                // caret over source the reader is not looking at, and a watch
                // that fails on every statement of a loop should be one line
                // per statement and not eight.
                let first = error
                    .to_string()
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .to_string();
                trace
                    .reports
                    .push(format!("watch {}: {}", watch.source, first));
            }
        }
    }
}

impl Repl {
    /// Drives a whole sub-REPL session from a list of lines, as the read loop
    /// drives the session's.
    ///
    /// The first line may be the `.debug` that opens it; every line after is
    /// the sub-REPL's, and a line that is not — asked before any session was
    /// open — is answered with [`NO_DEBUG_SESSION`] the way the parent answers
    /// it. What this returns is everything the lines answered, in order, which
    /// is what a test reads instead of a terminal.
    #[cfg(test)]
    pub(super) fn debug_session(&mut self, lines: &[&str]) -> Vec<String> {
        let mut answers = Vec::new();
        for line in lines {
            if self.debug_repl.is_some() {
                answers.extend(self.debug_line(line));
                continue;
            }
            match ReplCommand::parse(line) {
                ReplCommand::Debug(argument) => answers.extend(self.enter_debug(&argument)),
                _ => answers.push(NO_DEBUG_SESSION.to_string()),
            }
        }
        answers
    }

    /// Opens the sub-REPL on a fork of this session's VM and runs `argument`
    /// in it — a file when it names one, an expression or a block otherwise,
    /// and nothing at all when it is empty.
    ///
    /// The entry line is the parent's, asked at the parent's prompt; the
    /// session it opens is not. What runs here records no definitions, no
    /// transcript and no history: the sub-REPL is a copy on which to watch a
    /// program, not an extension of the session that opened it.
    pub(super) fn enter_debug(&mut self, argument: &str) -> Vec<String> {
        if self.debug_repl.is_some() {
            return vec![ALREADY_IN_SESSION.to_string()];
        }
        self.debug_repl = Some(DebugRepl::new(self.vm.fork(), self.style));
        let mut answers = vec![BANNER.to_string()];
        if !argument.trim().is_empty() {
            answers.extend(self.run_debug_argument(argument));
        }
        answers
    }

    /// One line inside the sub-REPL: a sub-REPL command, a program to run, or
    /// half of a block that is still being typed.
    pub(super) fn debug_line(&mut self, line: &str) -> Vec<String> {
        let Some(debug) = self.debug_repl.as_mut() else {
            return vec![NO_DEBUG_SESSION.to_string()];
        };
        match ReplCommand::parse(line) {
            ReplCommand::NotACommand => debug.push_line(line),
            ReplCommand::MissingArgument { command, usage } => {
                vec![format!("Usage: :{} {}", command, usage)]
            }
            ReplCommand::Unknown(word) => vec![format!(
                "Unknown command '{}'. Type :help for available commands.",
                word
            )],
            ReplCommand::Watch(source) => debug.watch(&source),
            ReplCommand::Stack => debug.stack_lines(),
            ReplCommand::Locals => debug.locals_lines(),
            ReplCommand::Help => vec![SUB_HELP.to_string()],
            ReplCommand::Run(path) => debug.run_file(&path),
            ReplCommand::Back => {
                self.debug_repl = None;
                Vec::new()
            }
            ReplCommand::Debug(argument) => {
                if argument.trim().is_empty() {
                    vec![ALREADY_IN_SESSION.to_string()]
                } else {
                    self.run_debug_argument(&argument)
                }
            }
            _ => vec![SUB_ONLY_MESSAGE.to_string()],
        }
    }

    /// The `.debug <argument>` half: the file to read or the code to run,
    /// answered whether or not a sub-REPL is open — `.debug prog.rb` at the
    /// parent prompt both opens one and runs the file in it.
    fn run_debug_argument(&mut self, argument: &str) -> Vec<String> {
        let argument = argument.trim();
        if names_a_file(argument) {
            self.run_debug_file(argument)
        } else {
            self.run_in_debug(argument, SESSION_FILE)
        }
    }

    /// Runs the file `path` names, in the open sub-REPL, under observation.
    fn run_debug_file(&mut self, path: &str) -> Vec<String> {
        match self.debug_repl.as_mut() {
            Some(debug) => debug.run_file(path),
            None => vec![NO_DEBUG_SESSION.to_string()],
        }
    }

    /// Runs `code` in the open sub-REPL, with no session to run it in being
    /// the answer rather than a panic.
    fn run_in_debug(&mut self, code: &str, file: &str) -> Vec<String> {
        match self.debug_repl.as_mut() {
            Some(debug) => debug.run(code, file),
            None => vec![NO_DEBUG_SESSION.to_string()],
        }
    }
}

/// Whether `argument` names a file to read rather than code to run.
///
/// Two rules, and the first one is the fallback:
///
/// - **A path that exists is a file**, whatever it is called. A file called
///   `notes.txt` holds Redblue source as readily as one called `prog.rb`, and
///   making the suffix the only way in would have meant the file could only be
///   debugged if it had been named by the convention rather than by the user.
/// - **A path ending in `.rb` is a file** even when it does not exist, because
///   a file that is missing is still a file the user meant, and saying "cannot
///   read" beats parsing `prog.rb` as an expression and failing on the dot.
///
/// Everything else is code, and is run as code.
fn names_a_file(argument: &str) -> bool {
    argument.ends_with(".rb") || std::path::Path::new(argument).is_file()
}
