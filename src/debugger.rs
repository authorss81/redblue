//! The debugger: breakpoints, stepping and frame inspection, observed through a
//! hook the VM calls before every statement.
//!
//! Nothing here is part of the language. There is no `break` statement, no
//! `step` keyword and no grammar change — the hook is asked, at each statement
//! boundary, whether to stop, and the VM evaluates exactly what it would have
//! evaluated with no debugger attached. The only Redblue-visible half is the
//! REPL's `:break` / `:step` / `:next` / `:continue` / `:frame` commands, which
//! reach this through [`ReplCommand`](crate::repl::commands::ReplCommand) and
//! cannot be written from Redblue source at all.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard};

use crate::value::{CapturedScope, Value};

/// Locks a session, ignoring a poisoned lock.
///
/// A hook that panicked while holding the lock has already failed the program
/// the user was debugging; refusing every later `:frame` because of it would
/// turn one failure into a session that can no longer be read at all.
pub fn lock(session: &SharedDebugSession) -> MutexGuard<'_, DebugSession> {
    session
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// What the debugger does at the next statement it is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepMode {
    /// Run until a breakpoint or a step is reached. Not stepping.
    None,
    /// Stop at the next statement at or *deeper* than the frame the step was
    /// asked in, so a call it makes is entered and its statements are shown one
    /// at a time.
    Into,
    /// Stop at the next statement at or *shallower* than the frame the step was
    /// asked in, so a call it makes is run to its end without stopping inside
    /// it.
    Over,
}

/// What the debugger asks the VM to do next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebugAction {
    /// Keep going until something else stops it.
    Run,
    /// Keep going until the next statement the mode names.
    Step(StepMode),
}

/// The hook the VM calls before every statement it is about to evaluate.
///
/// A plain boxed closure rather than a trait object with an implementation on
/// the VM: the debugger is one caller, and the hook is what a host installs to
/// be *told* what the program is doing rather than what it produced.
///
/// `Send` because the [`Vm`](crate::Vm) carrying it is sent to the sized thread
/// `rb run` executes a program on. A `DebugSession` the hook closes over is
/// therefore an [`Arc`] rather than an [`Rc`]; the REPL runs its program on its
/// own thread, and only the type has to satisfy the stricter rule.
pub type DebugHook = Box<dyn FnMut(DebugEvent<'_>) -> DebugAction + Send>;

/// One statement the VM is about to evaluate, as the debugger sees it.
pub struct DebugEvent<'a> {
    /// The 1-based source line the statement was written on.
    pub line: usize,
    /// How many function calls deep this statement is. Top level is `0`.
    pub depth: usize,
    /// How many statements are executing, this one included. Top level is `1`.
    ///
    /// What tells a statement nested inside the one that stopped from the same
    /// statement reached again later: a block written on a single line puts a
    /// `say` inside an `if` on the same line and the same depth, and a loop
    /// reaches its body line afresh on every iteration at the same depth.
    pub statement_depth: usize,
    /// The live local scopes, outermost first.
    pub scopes: &'a [CapturedScope],
    /// The program's globals, which top-level statements bind into rather than
    /// into `scopes`.
    pub globals: &'a HashMap<String, Value>,
    /// The names the VM was born holding. A frame lists what the program wrote,
    /// not the two hundred builtins it never touched.
    pub builtins: &'a HashSet<String>,
    /// The `Module_member` names a module's functions are published under — the
    /// interior of `MathUtils.circle_area`, which no line of Redblue can write.
    pub module_members: &'a HashSet<String>,
}

impl DebugEvent<'_> {
    /// Whether `name` is one the running program wrote, as opposed to something
    /// the VM was born holding.
    fn is_program_name(&self, name: &str) -> bool {
        !self.builtins.contains(name) && !self.module_members.contains(name)
    }
}

/// A frame as it was when the program stopped: a copy, because the values in it
/// keep changing as soon as the program is resumed.
#[derive(Debug, Clone)]
pub struct Frame {
    pub line: usize,
    pub depth: usize,
    /// Every name the program wrote that is visible at the stop, sorted by name
    /// so two stops with the same bindings list them the same way.
    pub bindings: Vec<(String, Value)>,
}

impl Frame {
    fn capture(event: &DebugEvent<'_>) -> Self {
        // The innermost scope that holds a name is the one a read would find, so
        // the walk runs from the inside out and the first sighting of a name
        // wins. Globals fill in the names no live scope holds.
        let mut bindings: Vec<(String, Value)> = Vec::new();
        let hold = |bindings: &mut Vec<(String, Value)>, name: &str, value: Value| {
            if !bindings.iter().any(|(held, _)| held == name) {
                bindings.push((name.to_string(), value));
            }
        };
        for scope in event.scopes.iter().rev() {
            for (name, value) in scope.iter() {
                hold(&mut bindings, name, value.clone());
            }
        }
        for (name, value) in event.globals {
            if event.is_program_name(name) {
                hold(&mut bindings, name, value.clone());
            }
        }
        bindings.sort_by(|left, right| left.0.cmp(&right.0));
        Frame {
            line: event.line,
            depth: event.depth,
            bindings,
        }
    }

    /// The frame as `line N (depth D)` followed by one indented `name = value`
    /// per binding — the listing `:frame` prints and the prompt shows.
    pub fn lines(&self) -> Vec<String> {
        let mut lines = vec![format!("line {} (depth {})", self.line, self.depth)];
        for (name, value) in &self.bindings {
            lines.push(format!("    {} = {}", name, value));
        }
        lines
    }
}

/// Where one stop was, in the three numbers that tell the ways the same line
/// comes round again from one another — see [`DebugSession::suppressed`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stop {
    line: usize,
    depth: usize,
    statement_depth: usize,
}

/// Everything a debugging session holds: where the breakpoints are, what the
/// next step should do, and the frame the program last stopped at.
///
/// Shared with the VM through an [`Arc`], because the hook the VM calls is
/// installed by the REPL that also has to answer `:frame` after the run.
pub type SharedDebugSession = Arc<Mutex<DebugSession>>;

#[derive(Debug, Clone)]
pub struct DebugSession {
    breakpoints: BTreeSet<usize>,
    pending: StepMode,
    /// The depth `pending` was asked at: the top-level statement the step was
    /// typed at, or `0` for a step typed at the prompt with nothing running.
    stop_depth: usize,
    /// Where the stop in progress was: the line, the call depth, and how deep in
    /// statements it was. A breakpoint on that line is not reported again while
    /// execution is still *inside* that statement — a block written on one line
    /// stops once rather than once per statement in it.
    ///
    /// All three are needed to tell the three ways the same line comes round
    /// again. Same call depth and deeper into statements: a nested statement of
    /// the block being run, which is not the line being reached. Deeper call
    /// depth: a recursive call reaching the line afresh, which is. The same
    /// statement depth in the same call: a loop body on its next iteration,
    /// which is too.
    suppressed: Option<Stop>,
    /// The source the program being run was written in, by line, so a stop can
    /// show the line it stopped on. Empty when there is nothing to show.
    source: Vec<String>,
    last_stop: Option<Frame>,
}

impl Default for DebugSession {
    fn default() -> Self {
        Self::new()
    }
}

impl DebugSession {
    pub fn new() -> Self {
        DebugSession {
            breakpoints: BTreeSet::new(),
            pending: StepMode::None,
            stop_depth: 0,
            suppressed: None,
            source: Vec::new(),
            last_stop: None,
        }
    }

    /// A session the hook and its caller share.
    pub fn shared() -> SharedDebugSession {
        Arc::new(Mutex::new(Self::new()))
    }

    /// Whether the VM has to be asked about the next statement at all.
    ///
    /// False for a session with no breakpoint and no step pending, which is
    /// every session that has not asked to debug anything: the hook is not even
    /// installed then, so a program run without a debugger pays one predictable
    /// branch and nothing else.
    pub fn active(&self) -> bool {
        !self.breakpoints.is_empty() || self.pending != StepMode::None
    }

    /// Where the breakpoints are, ascending. A `BTreeSet` so the listing is
    /// ordered by the line number rather than by insertion or hash order.
    pub fn breakpoints(&self) -> Vec<usize> {
        self.breakpoints.iter().copied().collect()
    }

    /// Sets a breakpoint on `line`, or clears it if it was already set.
    ///
    /// Returns whether the line is a breakpoint afterwards, so `:break 3` twice
    /// removes it — the toggle every debugger has, and the reason a user who
    /// mis-set a breakpoint is not stuck with it.
    pub fn toggle_breakpoint(&mut self, line: usize) -> bool {
        if self.breakpoints.contains(&line) {
            self.breakpoints.remove(&line);
            false
        } else {
            self.breakpoints.insert(line);
            true
        }
    }

    /// Removes the breakpoint on `line`, reporting whether there was one.
    pub fn clear_breakpoint(&mut self, line: usize) -> bool {
        self.breakpoints.remove(&line)
    }

    /// Arms a step for the *next* program the session runs.
    ///
    /// At the prompt there is no frame to step out of, so the step starts at
    /// depth `0` and stops at the first statement of the program.
    pub fn arm_step(&mut self, mode: StepMode) {
        self.pending = mode;
        self.stop_depth = 0;
    }

    /// The source the program being run was written in, for showing a line.
    pub fn set_source(&mut self, source: &str) {
        self.source = source.lines().map(|line| line.to_string()).collect();
    }

    /// The text of `line`, or an empty string when there is no source to read it
    /// from — a REPL line with no file behind it still has its own text, but a
    /// program the debugger was never shown must not invent one.
    fn source_line(&self, line: usize) -> String {
        if line == 0 {
            return String::new();
        }
        self.source.get(line - 1).cloned().unwrap_or_default()
    }

    /// Decides whether the VM should stop before the statement `line` runs, and
    /// records the frame if it should.
    ///
    /// The answer is the only thing the hook gets, and returning `false` is the
    /// whole of "carry on": the VM then evaluates what it was going to evaluate.
    pub fn poll(&mut self, event: &DebugEvent<'_>) -> bool {
        let (line, depth) = (event.line, event.depth);
        let here = Stop {
            line,
            depth,
            statement_depth: event.statement_depth,
        };
        // Inside the statement that already stopped, on the same line and in the
        // same call? Then this is a nested statement of it, not the line being
        // reached again — see `DebugSession::suppressed`.
        let inside_stop = self.suppressed.is_some_and(|stopped| {
            stopped.line == here.line
                && stopped.depth == here.depth
                && stopped.statement_depth < here.statement_depth
        });

        let stepping = match self.pending {
            StepMode::None => false,
            StepMode::Into => depth >= self.stop_depth,
            StepMode::Over => depth <= self.stop_depth,
        };
        let breakpoint = !inside_stop && self.breakpoints.contains(&line);

        if !stepping && !breakpoint {
            return false;
        }

        self.pending = StepMode::None;
        self.stop_depth = depth;
        self.suppressed = Some(here);
        self.last_stop = Some(Frame::capture(event));
        true
    }

    /// The answer the VM acts on after a stop: what `c` / `n` / `s` at the
    /// debugger prompt asked for.
    pub fn record_action(&mut self, action: DebugAction) {
        match action {
            DebugAction::Run => {
                self.pending = StepMode::None;
            }
            DebugAction::Step(mode) => {
                // The step starts from the statement it was typed at, so
                // `next` at depth 0 runs the call it is looking at to the end.
                self.pending = mode;
                self.stop_depth = self.last_stop.as_ref().map_or(0, |frame| frame.depth);
            }
        }
    }

    /// The frame of the last stop, or `None` when nothing has stopped yet.
    pub fn last_stop(&self) -> Option<&Frame> {
        self.last_stop.as_ref()
    }

    /// What a stop looks like in the session: the source line, then the frame.
    pub fn stop_report(&self) -> Vec<String> {
        let Some(frame) = &self.last_stop else {
            return Vec::new();
        };
        let mut lines = Vec::new();
        let source = self.source_line(frame.line);
        if source.is_empty() {
            lines.push(format!("Stopped at line {}", frame.line));
        } else {
            lines.push(format!("Stopped at line {}: {}", frame.line, source.trim()));
        }
        lines.extend(frame.lines());
        lines
    }
}
