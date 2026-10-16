//! The typed value a `catch error of <Type>` matches against.
//!
//! [`Error`](crate::error::Error) is the host's own error type: `Runtime`,
//! `Io`, `Limit` and the three compile-time kinds, in Rust, for the host and
//! its tooling. None of that reaches a Redblue program — a `catch` used to bind
//! the untyped text `error`, so there was nothing a handler could ask about the
//! failure it had caught and nothing for `of <Type>` to match.
//!
//! This module is the boundary between the two. It answers one question —
//! *which of the kinds a program may name is this failure?* — from an
//! [`Error`], and it builds the one value shape a matching handler binds: a
//! record of `kind`, `message`, `line` and `column`.
//!
//! Both engines call the same two functions, so a failure caught on the
//! tree-walking VM and the same failure caught on the bytecode VM are built the
//! same way: the same four fields, in the same order, by the same code. What
//! they are not is the same *failure* — the two engines place a runtime failure
//! at different columns by their own convention, because a `.rbc` instruction
//! carries a line and no column (`src/bytecode/vm.rs`, `line_span`). So the
//! `kind` and the `message` a handler reads are the same on both engines, and
//! the `line` is too, while `column` says where each engine says it happened. A
//! program that compares the whole record across the two is comparing two
//! different failures; one that reads `kind` is not.
//!
//! The list is closed and small. A program cannot invent a kind, cannot raise
//! one, and cannot match `Io` against a failure that never touched the host:
//! three names, plus `Error` for the handler that wants all of them.

use crate::error::Error;
use crate::value::{Fields, Value};

/// A kind of failure a `catch error of <Type>` can name.
///
/// Three kinds reach a program, and one name covers all three. The host's other
/// kinds — [`Error::Lexer`], [`Error::Parser`] and [`Error::Analyzer`] — are
/// raised before a program runs, so there is no `try` for them to be caught
/// inside; they are refused by the parser rather than named here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    /// `Error` — every kind below. The handler that asks for "whatever this
    /// was", and the typed spelling of what `catch error` already does.
    Any,
    /// `Runtime` — the program's own expression went wrong: a type mismatch, an
    /// index past the end of a list, a division by zero, a name that is not
    /// bound.
    Runtime,
    /// `Io` — the host's filesystem and environment: a file that is not there,
    /// a path that cannot be read, a module file that cannot be loaded.
    Io,
    /// `Limit` — one of the host's resource limits: the step budget, the
    /// call-depth limit, or a loop's iteration cap.
    ///
    /// A kind of its own rather than a `Runtime` that reads like one, because
    /// `might fail` answers differently for the two and a program that has to
    /// tell them apart should not be matching on a message to do it. See
    /// [`Error::is_resource_limit`].
    Limit,
}

/// The kinds a program may name, in the order [`FailureKind::names`] reports
/// them, and the only kinds the parser will accept after `of`.
pub const KINDS: [FailureKind; 4] = [
    FailureKind::Any,
    FailureKind::Runtime,
    FailureKind::Io,
    FailureKind::Limit,
];

impl FailureKind {
    /// The name this kind is spelled in a program: `Error`, `Runtime`, `Io` or
    /// `Limit`.
    pub fn name(self) -> &'static str {
        match self {
            FailureKind::Any => "Error",
            FailureKind::Runtime => "Runtime",
            FailureKind::Io => "Io",
            FailureKind::Limit => "Limit",
        }
    }

    /// The kind `name` names, or `None` for anything else.
    ///
    /// The closed list is enforced here, so an invented kind is refused by the
    /// parser rather than compiled into a handler that matches nothing.
    pub fn from_name(name: &str) -> Option<FailureKind> {
        KINDS.into_iter().find(|kind| kind.name() == name)
    }

    /// Every name a program may write, joined by `, `.
    ///
    /// The list an unknown-kind error quotes, so the refusal names the kinds
    /// that do exist rather than only the one that does not.
    pub fn names() -> String {
        KINDS
            .iter()
            .map(|kind| kind.name())
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Whether a handler naming this kind runs for `error`.
    pub fn matches(self, error: &Error) -> bool {
        match self {
            FailureKind::Any => true,
            other => other == kind_of(error),
        }
    }
}

/// The kind of `error` as a program sees it.
///
/// A failure the host raised before the program ran has no name a program could
/// have matched, and is reported as a `Runtime` one: it is a failure of the
/// program's own text rather than of the host it asked something of. Nothing
/// routes one here — a `try` is installed by instructions, and an instruction
/// only runs once the program has passed the frontend — so this is the answer
/// for a failure that cannot happen rather than a case a program can reach.
fn kind_of(error: &Error) -> FailureKind {
    match error {
        Error::Io(_) => FailureKind::Io,
        Error::Limit(_, _) => FailureKind::Limit,
        Error::Lexer(_, _) | Error::Parser(_, _) | Error::Analyzer(_, _) | Error::Runtime(_, _) => {
            FailureKind::Runtime
        }
    }
}

/// The value a matching handler binds: a record of the failure's kind, its
/// message and where it happened.
///
/// A record and not a variant of [`Value`], because `Value`'s variants are the
/// language's public shape (`phases/INVARIANTS.md`) and a tenth one would be a
/// language change this phase is not making. A record is also what a program
/// can already do everything with: read a field of it, put it in a list, print
/// it, hand it to a function.
///
/// `line` and `column` are `0` when the failure has no position — an `Io`
/// failure raised before any token of the program has been read, or one the
/// host raised rather than the program. `0` is a line no source file has, so it
/// reads as "nowhere" without a second spelling.
pub fn value_of(error: &Error) -> Value {
    let span = error.span();
    let (line, column) = match span {
        Some(span) => (span.line, span.column),
        None => (0, 0),
    };

    let mut fields = Fields::new();
    fields.insert(
        "kind".to_string(),
        Value::Text(kind_of(error).name().to_string()),
    );
    fields.insert(
        "message".to_string(),
        Value::Text(error.message().to_string()),
    );
    fields.insert("line".to_string(), position(line));
    fields.insert("column".to_string(), position(column));

    Value::record(fields)
}

/// One end of a position as a number.
///
/// Built through the same arithmetic every other number in the language is, so
/// a line past `usize` cannot become a value the language has no spelling for;
/// a position that large is not one a source file has.
///
/// Saturating rather than `as`, because the narrowing conversion of a `usize`
/// past `2^53` is not a rounding the language can name: every number it holds is
/// an `f64`, and the largest one it can write back out is `2^53`. A line that
/// big is not a line, so the position saturates at the largest number there is
/// rather than becoming a different one.
fn position(n: usize) -> Value {
    const LARGEST: f64 = 9_007_199_254_740_992.0; // 2^53
    let as_number = n as f64;
    if as_number > LARGEST {
        Value::Number(LARGEST)
    } else {
        Value::Number(as_number)
    }
}

/// The kind `error` is, for a caller that is not a handler: the linter, the
/// debugger, the REPL. Public so that every place that shows a failure a
/// program could have caught shows the same name for it.
pub fn kind_name(error: &Error) -> &'static str {
    kind_of(error).name()
}
