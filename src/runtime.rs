//! What a Redblue *operation* means, in the one place both VMs read it.
//!
//! Two VMs now exist: the tree-walker in [`crate::vm`], which is what `rb run`
//! uses and what the language is specified against, and the bytecode VM in
//! [`crate::bytecode::vm`], which runs a `.rbc`. They must agree on what
//! `1 + 2` is, what `xs[9]` says when `xs` holds one element, and what
//! `files.read` does — down to the wording of the failure.
//!
//! Rather than write that twice and keep the two copies in step by hand, every
//! operation both VMs need is here: the arithmetic and comparison operators,
//! indexing, property access, the builtin library, and the JSON and CSV
//! readers behind it. A VM decides *how* to reach an operation — by walking an
//! AST or by running an instruction — and this module says what it means.
//!
//! The tree-walker is the reference: what is here is what it already did,
//! moved rather than rewritten, so `rb run` behaves exactly as before.

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::format::{Item, StrftimeItems};
use chrono::{Datelike, Months, NaiveDate, NaiveDateTime, NaiveTime, TimeDelta, Timelike};

use crate::error::{Error, Result, Span};
use crate::lexer::Lexer;
use crate::parser::{BinaryOp, Expr, Program, Statement, UnaryOp};
use crate::value::{Fields, Value};

/// Parses a module file into the program its top level declares.
///
/// The caller keeps the `Program` to remember that the module has already been
/// loaded, so a second `import` of the same name is a no-op rather than a second
/// binding of the same names. [`module_bindings`] reads the declarations out of
/// the same `Program`, so a module is read and parsed once however many names it
/// contributes.
///
/// Through [`crate::vfs`], not `std::fs`: on `wasm32-unknown-unknown` a module
/// is a file in the same in-memory filesystem `files.write` writes to, so a
/// program that writes a module and then imports it works. A `std::fs` read
/// would compile, pass every native test, and fail there.
pub fn module_program(path: &str) -> Result<Program> {
    let source = crate::vfs::read_to_string(path)
        .map_err(|e| Error::Io(format!("Cannot load module '{}': {}", path, e)))?;

    let tokens = Lexer::tokenize(&source)?;
    crate::parser::Parser::new(tokens).parse()
}

/// The names `program` binds: one per top-level `set` or `constant`.
///
/// The caller binds them through whatever refusal its own name resolution uses,
/// so a module's `constant` is refused a second binding exactly as a `constant`
/// written in the importing program would be. Returning the kind alongside the
/// name is what lets the caller choose between its two binding paths.
///
/// Takes the parsed program rather than the path so that the file is read once:
/// reading it again could fail — the file could have gone away between the two
/// reads — after the caller had already installed some of the bindings.
pub fn module_bindings(
    program: &Program,
    mut evaluate: impl FnMut(&Expr) -> Result<Value>,
) -> Result<Vec<(String, Value, bool)>> {
    // A module's functions are not bound to a name, so a member stays
    // unreachable — see FINDINGS.md. The two declarations below are the whole of
    // what an import currently contributes.
    let mut bound = Vec::new();
    for stmt in crate::parser::module_body(program) {
        let (name, value, is_const) = match &stmt.statement {
            Statement::Set { name, value } => (name, value, false),
            Statement::Constant { name, value } => (name, value, true),
            _ => continue,
        };
        bound.push((name.clone(), evaluate(value)?, is_const));
    }

    Ok(bound)
}

/// `object.property`, and the failure for a receiver that is not a record.
pub fn property(span: Span, object: Value, name: &str) -> Result<Value> {
    match object {
        Value::Record(fields) => Ok(fields.get(name).cloned().unwrap_or(Value::Nothing)),
        Value::Date(date) => Ok(date_property(date, name)),
        Value::Duration(duration) => Ok(duration_property(duration, name)),
        _ => Err(Error::Runtime(
            "Cannot access property on non-object".to_string(),
            span,
        )),
    }
}

/// One named component of a date, or `nothing` for a name it does not have.
///
/// A missing property answers `nothing` for the same reason a record does: a
/// program can test for it, and a typo is a value it can see rather than an
/// error that stops the run.
fn date_property(date: NaiveDateTime, name: &str) -> Value {
    let number = match name {
        "year" => date.year() as f64,
        "month" => date.month() as f64,
        "day" => date.day() as f64,
        "hour" => date.hour() as f64,
        "minute" => date.minute() as f64,
        "second" => date.second() as f64,
        "weekday" => date.weekday().number_from_monday() as f64,
        "nanosecond" => date.nanosecond() as f64,
        _ => return Value::Nothing,
    };
    Value::Number(number)
}

/// One named component of a duration, or `nothing` for a name it does not have.
///
/// `days`, `hours`, `minutes` and `seconds` are the components a reader sees:
/// the hours are the remainder inside the last day, and so on, so
/// `time.duration(90000)` reads as `1 day, 1 hour`. `total_seconds` is the whole
/// span, and `nanoseconds` is the fractional second that is left.
fn duration_property(duration: TimeDelta, name: &str) -> Value {
    let number = match name {
        "days" => duration.num_days() as f64,
        "hours" => (duration.num_hours() % 24) as f64,
        "minutes" => (duration.num_minutes() % 60) as f64,
        "seconds" => (duration.num_seconds() % 60) as f64,
        "total_seconds" => duration.as_seconds_f64(),
        // `subsec_nanos` is already signed: it is the fractional second with
        // the span's sign, so negating a negative span here turned -0.5
        // seconds into a positive half second that no longer summed to the
        // total.
        "nanoseconds" => duration.subsec_nanos() as f64,
        _ => return Value::Nothing,
    };
    Value::Number(number)
}

/// `list[index]`, and every failure a bad index is.
pub fn index(span: Span, list: Value, index_value: Value) -> Result<Value> {
    let Value::List(items) = list else {
        return Err(Error::Runtime("Cannot index non-list".to_string(), span));
    };
    let Value::Number(n) = index_value else {
        return Err(Error::Runtime("Index must be a number".to_string(), span));
    };
    // A fractional index names no element, so it is rejected rather than
    // truncated: `items[0.5]` must not quietly answer `items[0]`.
    if n.fract() != 0.0 {
        return Err(Error::Runtime(
            format!(
                "Index {} is out of bounds: a list index must be a whole number",
                Value::Number(n)
            ),
            span,
        ));
    }
    // A negative index counts from the end, so `n as i64` saturates to
    // `i64::MIN` at one extreme. That sum is in range while `len` is
    // non-negative, but the bound is arithmetic rather than a stated
    // invariant, so it is checked instead of assumed, and the result is
    // converted rather than cast: a negative offset cast to `usize` wraps to a
    // huge value and would name a different element than the program asked for.
    let element = if n < 0.0 {
        (items.len() as i64).checked_add(n as i64)
    } else {
        Some(n as i64)
    }
    .and_then(|offset| usize::try_from(offset).ok())
    .and_then(|offset| items.get(offset))
    .cloned();
    element.ok_or_else(|| {
        Error::Runtime(
            format!(
                "Index {} is out of bounds: length is {}, {}",
                Value::Number(n),
                items.len(),
                if items.is_empty() {
                    "the list is empty, so it has no valid index".to_string()
                } else {
                    format!("valid indexes are 0 to {}", items.len() - 1)
                }
            ),
            span,
        )
    })
}

/// The value of a binary operator, and the failure for the wrong types.
pub fn binary_op(span: Span, op: &BinaryOp, left: Value, right: Value) -> Result<Value> {
    match op {
        BinaryOp::Add => match (left, right) {
            (Value::Number(a), Value::Number(b)) => Value::number(a + b, span),
            (Value::Text(a), Value::Text(b)) => Ok(Value::Text(format!("{}{}", a, b))),
            (Value::Date(date), Value::Duration(delta)) => date_add(span, date, delta),
            (Value::Duration(delta), Value::Date(date)) => date_add(span, date, delta),
            (Value::Duration(a), Value::Duration(b)) => a
                .checked_add(&b)
                .map(Value::Duration)
                .ok_or_else(|| Error::Runtime("The duration is too large".to_string(), span)),
            (Value::Date(_), Value::Date(_)) => Err(Error::Runtime(
                "Cannot add two dates; add a duration to a date".to_string(),
                span,
            )),
            _ => Err(Error::Runtime("Cannot add non-numbers".to_string(), span)),
        },
        BinaryOp::Sub => match (left, right) {
            (Value::Number(a), Value::Number(b)) => Value::number(a - b, span),
            (Value::Date(a), Value::Date(b)) => Ok(Value::Duration(a - b)),
            (Value::Date(date), Value::Duration(delta)) => date_sub(span, date, delta),
            (Value::Duration(a), Value::Duration(b)) => a
                .checked_sub(&b)
                .map(Value::Duration)
                .ok_or_else(|| Error::Runtime("The duration is too large".to_string(), span)),
            (Value::Duration(_), Value::Date(_)) => Err(Error::Runtime(
                "Cannot subtract a date from a duration".to_string(),
                span,
            )),
            _ => Err(Error::Runtime(
                "Cannot subtract non-numbers".to_string(),
                span,
            )),
        },
        BinaryOp::Mul => {
            if let (Value::Number(a), Value::Number(b)) = (left, right) {
                Value::number(a * b, span)
            } else {
                Err(Error::Runtime(
                    "Cannot multiply non-numbers".to_string(),
                    span,
                ))
            }
        }
        BinaryOp::Div => {
            if let (Value::Number(a), Value::Number(b)) = (left, right) {
                if b == 0.0 {
                    Err(Error::Runtime("Division by zero".to_string(), span))
                } else {
                    Value::number(a / b, span)
                }
            } else {
                Err(Error::Runtime(
                    "Cannot divide non-numbers".to_string(),
                    span,
                ))
            }
        }
        BinaryOp::Mod => {
            if let (Value::Number(a), Value::Number(b)) = (left, right) {
                if b == 0.0 {
                    Err(Error::Runtime("Modulo by zero".to_string(), span))
                } else {
                    Value::number(a % b, span)
                }
            } else {
                Err(Error::Runtime(
                    "Cannot modulo non-numbers".to_string(),
                    span,
                ))
            }
        }
        BinaryOp::Equal => equality(span, left, right, false),
        BinaryOp::NotEqual => equality(span, left, right, true),
        BinaryOp::Less => compare(span, op, left, right),
        BinaryOp::LessEqual => compare(span, op, left, right),
        BinaryOp::Greater => compare(span, op, left, right),
        BinaryOp::GreaterEqual => compare(span, op, left, right),
        BinaryOp::And => Ok(Value::YesNo(left.is_truthy() && right.is_truthy())),
        BinaryOp::Or => Ok(Value::YesNo(left.is_truthy() || right.is_truthy())),
        BinaryOp::In => {
            if let Value::List(items) = right {
                Ok(Value::YesNo(items.contains(&left)))
            } else {
                Err(Error::Runtime(
                    "Right side of 'in' must be a list".to_string(),
                    span,
                ))
            }
        }
    }
}

/// `date + duration` and `duration + date`, refusing a span that leaves the
/// calendar.
fn date_add(span: Span, date: NaiveDateTime, delta: TimeDelta) -> Result<Value> {
    date.checked_add_signed(delta)
        .map(Value::Date)
        .ok_or_else(|| {
            Error::Runtime(
                "The date is too far from the calendar this language can name".to_string(),
                span,
            )
        })
}

/// `date - duration`, refusing a span that leaves the calendar.
fn date_sub(span: Span, date: NaiveDateTime, delta: TimeDelta) -> Result<Value> {
    date.checked_sub_signed(delta)
        .map(Value::Date)
        .ok_or_else(|| {
            Error::Runtime(
                "The date is too far from the calendar this language can name".to_string(),
                span,
            )
        })
}

/// `is` and `is not`, refusing a date or a duration mixed with a number.
///
/// Ordering already treats the two kinds as incomparable, and equality is the
/// same question: `date is 5` used to answer a silent `no`, which looks like a
/// real answer to a program that compared the wrong kinds. The refusal names
/// both kinds, exactly as ordering does, so a `try … catch error of Runtime`
/// can report it.
fn equality(span: Span, left: Value, right: Value, negated: bool) -> Result<Value> {
    let mixed = match (&left, &right) {
        (Value::Date(_), Value::Number(_)) => Some(("date", "number")),
        (Value::Number(_), Value::Date(_)) => Some(("number", "date")),
        (Value::Duration(_), Value::Number(_)) => Some(("duration", "number")),
        (Value::Number(_), Value::Duration(_)) => Some(("number", "duration")),
        _ => None,
    };
    if let Some((left_kind, right_kind)) = mixed {
        return Err(Error::Runtime(
            format!("Cannot compare a {left_kind} with a {right_kind}"),
            span,
        ));
    }
    let equal = left == right;
    Ok(Value::YesNo(if negated { !equal } else { equal }))
}

/// `<`, `<=`, `>` and `>=` over numbers, dates and durations.
///
/// A date and a number are different kinds of value, so ordering them is a
/// refusal rather than a coercion: a program that wrote `date < 5` asked a
/// question with no answer, and it is told which two kinds it mixed.
fn compare(span: Span, op: &BinaryOp, left: Value, right: Value) -> Result<Value> {
    use std::cmp::Ordering;

    let ordering = match (&left, &right) {
        (Value::Number(a), Value::Number(b)) => a.partial_cmp(b),
        (Value::Date(a), Value::Date(b)) => Some(a.cmp(b)),
        (Value::Duration(a), Value::Duration(b)) => Some(a.cmp(b)),
        (Value::Date(_), other) => {
            return Err(Error::Runtime(
                format!("Cannot compare a date with a {}", other.type_name()),
                span,
            ))
        }
        (other, Value::Date(_)) => {
            return Err(Error::Runtime(
                format!("Cannot compare a {} with a date", other.type_name()),
                span,
            ))
        }
        (Value::Duration(_), other) => {
            return Err(Error::Runtime(
                format!("Cannot compare a duration with a {}", other.type_name()),
                span,
            ))
        }
        (other, Value::Duration(_)) => {
            return Err(Error::Runtime(
                format!("Cannot compare a {} with a duration", other.type_name()),
                span,
            ))
        }
        _ => {
            return Err(Error::Runtime(
                "Cannot compare non-numbers".to_string(),
                span,
            ))
        }
    };
    let ordering =
        ordering.ok_or_else(|| Error::Runtime("Cannot compare these numbers".to_string(), span))?;
    let answer = match op {
        BinaryOp::Less => ordering == Ordering::Less,
        BinaryOp::LessEqual => ordering != Ordering::Greater,
        BinaryOp::Greater => ordering == Ordering::Greater,
        BinaryOp::GreaterEqual => ordering != Ordering::Less,
        _ => {
            return Err(Error::Runtime(
                "Cannot compare these values".to_string(),
                span,
            ))
        }
    };
    Ok(Value::YesNo(answer))
}

/// The value of a unary operator, and the failure for the wrong type.
pub fn unary_op(span: Span, op: &UnaryOp, value: Value) -> Result<Value> {
    match op {
        UnaryOp::Neg => {
            if let Value::Number(n) = value {
                Ok(Value::Number(-n))
            } else {
                Err(Error::Runtime("Cannot negate non-number".to_string(), span))
            }
        }
        UnaryOp::Not => Ok(Value::YesNo(!value.is_truthy())),
    }
}

/// `append(name, value)` with the name lookup folded in: given the binding a
/// read of `name` resolves to, this grows it and returns.
///
/// `current` is `None` for a name no scope holds, and a `&mut` for one that is.
/// Taking the binding by reference rather than by value is the whole of what
/// makes the append a step: a value read *out* of a scope carries a second
/// handle on the storage, and `make_mut` answers a second handle with a copy.
/// Growing through the binding itself is the only way to see that the caller is
/// the one holding it.
///
/// A name that holds something other than a list is refused rather than bound
/// over: `append` grows a list, and replacing a number with a list would make it
/// a `set` that says nothing about what it did.
pub fn append_through(
    span: Span,
    name: &str,
    current: Option<&mut Value>,
    value: Value,
) -> Result<()> {
    let Some(list) = current else {
        return Err(Error::Runtime(
            format!("Cannot append to '{name}': no name '{name}' is bound to a list"),
            span,
        ));
    };
    if !list.append(value) {
        return Err(Error::Runtime(
            format!(
                "Cannot append to '{name}': it holds a {}, not a list",
                list.type_name()
            ),
            span,
        ));
    }
    Ok(())
}

/// The name `append` grows, and the refusal for a call that did not give one.
pub fn append_target(span: Span, args: &[Value]) -> Result<&str> {
    match (args.first(), args.len()) {
        (Some(Value::Text(name)), 2) => Ok(name),
        _ => Err(Error::Runtime(
            "append requires a name and a value, as in append(\"xs\", 1)".to_string(),
            span,
        )),
    }
}

/// How long `instant` is after the Unix epoch, to nanosecond precision.
///
/// `SystemTime::duration_since` is `Err` for any instant before
/// 1970-01-01T00:00:00Z, which is a clock set backwards, and `Err` must not be
/// an `unwrap`: an `unwrap` here unwinds the interpreter thread, so a Redblue
/// program that merely called `time.now()` would abort the process instead of
/// getting an error it could `catch`. The message names the clock rather than
/// the underlying `SystemTimeError`, whose text ("second time provided was
/// later than self") says nothing about what the program did.
///
/// The instant is a parameter rather than a `SystemTime::now()` call inside,
/// so the pre-epoch path is reachable from a test that passes the instant
/// directly. Without that seam the only way to exercise it is to set the
/// machine's clock, which no test may do.
fn since_epoch(instant: SystemTime, span: Span) -> Result<Duration> {
    instant.duration_since(UNIX_EPOCH).map_err(|_| {
        Error::Runtime(
            "The system clock is set before 1970-01-01T00:00:00Z, so time cannot be read from it"
                .to_string(),
            span,
        )
    })
}

/// The current instant, as the `time` module reads it.
///
/// `SystemTime::now()` panics on `wasm32-unknown-unknown` — that platform has
/// no clock for a program to read — and a panic there aborts the whole module
/// instance rather than failing one run, so a playground whose `time.now()`
/// took the page down with it would be worse than one that says no.
///
/// The playground asks the host for the time instead: `redblue.js` passes the
/// browser's `Date.now()` through `rb_set_clock` before each run, and a host
/// that does not is told so. This is a source of a value, like the filesystem
/// and the network: nothing about how `time` is *used* differs, and
/// `time.format`, `time.unix` and the arithmetic over a timestamp are the same
/// code in both builds.
#[cfg(not(target_arch = "wasm32"))]
fn now() -> Result<SystemTime> {
    Ok(SystemTime::now())
}

/// The playground's clock, as a [`Duration`] from the Unix epoch. `None` until
/// the host has supplied one.
#[cfg(target_arch = "wasm32")]
static HOST_CLOCK: std::sync::Mutex<Option<Duration>> = std::sync::Mutex::new(None);

/// Records the host's clock. Exported as `rb_set_clock`; see [`now`].
#[cfg(target_arch = "wasm32")]
pub fn set_host_clock(seconds: f64) {
    if let Ok(mut slot) = HOST_CLOCK.lock() {
        *slot = if seconds.is_finite() && seconds >= 0.0 {
            Some(Duration::from_secs_f64(seconds))
        } else {
            None
        };
    }
}

#[cfg(target_arch = "wasm32")]
fn now() -> Result<SystemTime> {
    let slot = HOST_CLOCK
        .lock()
        .map_err(|_| Error::Io("The playground clock could not be read".to_string()))?;
    let since_epoch = slot.ok_or_else(|| {
        Error::Runtime(
            "time.now() has no clock in the WebAssembly playground: the platform has none for \
             a program to read. The host supplies one — see rb_set_clock in src/wasm.rs."
                .to_string(),
            Span::unknown(),
        )
    })?;

    // `UNIX_EPOCH + d` panics on overflow, and the host's clock is an input a
    // page can set to anything, so the round trip is checked rather than
    // trusted: a host that sends 1e300 gets a clean error, not an abort.
    UNIX_EPOCH.checked_add(since_epoch).ok_or_else(|| {
        Error::Runtime(
            format!(
                "The playground's clock is at {since_epoch:?}, which is not an instant \
                     this platform can represent"
            ),
            Span::unknown(),
        )
    })
}

/// The whole seconds `timestamp` names, counted from the Unix epoch, for
/// `chrono` to format.
///
/// `SystemTime + Duration` panics when the addition overflows, and it does so
/// from inside `std`, past every `Result` in this crate — `time.format(1e300)`
/// aborted the process. The range is therefore checked here: a timestamp that
/// is not finite or past what a date can name is a `Runtime` error, while a
/// fractional timestamp truncates toward zero instead of being refused.
/// `timestamp as u64` is not usable for the conversion: it saturates, so
/// a negative timestamp would have quietly formatted as the epoch rather than
/// as 1969.
fn seconds_for_format(timestamp: f64, span: Span) -> Result<i64> {
    seconds_for_instant(timestamp, "time.format", span)
}

/// The whole seconds a builtin that names an instant should read.
///
/// Shared by `time.format` and `time.from_unix`: both take a number counted
/// from the epoch and both must agree on which instants exist. The builtin's
/// name is a parameter so a refusal says which call the program wrote.
fn seconds_for_instant(timestamp: f64, builtin: &str, span: Span) -> Result<i64> {
    if !timestamp.is_finite() {
        return Err(Error::Runtime(
            format!("{builtin} requires a finite number of seconds, got {timestamp}"),
            span,
        ));
    }
    // A timestamp with a fractional part keeps the behaviour it always had:
    // `as` truncates toward zero, so `time.format(1.5)` is one second after
    // the epoch and `time.format(-0.5)` is the epoch itself. Flooring a
    // negative fraction instead moved every pre-epoch fraction one second
    // earlier than `time.format` and `time.from_unix` answered before
    // phase-097, which is a compatibility regression rather than a fix: the
    // phase's contract is that the inputs these builtins already handled keep
    // their answers. `time.unix` still answers a fractional number for a date
    // that holds a fraction, so the pre-epoch instant is not lost; only the
    // legacy whole-second conversion of a negative number stays truncated.
    // `i64::MAX` seconds is far past the last instant a `SystemTime` can hold,
    // and `chrono` cannot format one either, so the range is bounded here
    // rather than by whichever of the two overflows first. A timestamp before
    // the epoch is *not* out of range: 1969 is a real year, and this function
    // hands the seconds back for `chrono` to format, so `time.format(-1)` is
    // 1969-12-31 rather than an error.
    const MAX_SECONDS: f64 = 253_402_300_800.0; // 10000-01-01T00:00:00Z
    if timestamp < -MAX_SECONDS || timestamp > MAX_SECONDS {
        return Err(Error::Runtime(
            format!("{builtin} cannot represent {timestamp} seconds from 1970-01-01T00:00:00Z"),
            span,
        ));
    }
    Ok(timestamp as i64)
}

/// A refusal from the one text-to-date parser, naming where it stopped.
///
/// The position is a byte offset into `text`, which is also a character offset
/// because every accepted byte is ASCII. The offending characters are quoted
/// with `text.get`, so a position inside a multi-byte character is reported
/// without slicing through it.
fn date_parse_error(text: &str, pos: usize, builtin: &str, expected: &str, span: Span) -> Error {
    let found = text
        .get(pos..)
        .map(|rest| rest.chars().take(12).collect::<String>())
        .unwrap_or_default();
    let found = if found.is_empty() {
        "the end of the text".to_string()
    } else {
        format!("`{found}`")
    };
    Error::Runtime(
        format!("{builtin} could not parse at position {pos}: expected {expected}, found {found}"),
        span,
    )
}

/// Reads exactly `count` ASCII digits at `pos`, leaving `pos` after them.
fn date_digits(
    text: &str,
    pos: &mut usize,
    count: usize,
    label: &str,
    builtin: &str,
    span: Span,
) -> Result<u32> {
    let bytes = text.as_bytes();
    let start = *pos;
    let mut value: u32 = 0;
    for offset in 0..count {
        let Some(byte) = bytes.get(start + offset) else {
            return Err(date_parse_error(
                text,
                start,
                builtin,
                &format!("{count} digits for the {label}"),
                span,
            ));
        };
        if !byte.is_ascii_digit() {
            return Err(date_parse_error(
                text,
                start + offset,
                builtin,
                &format!("{count} digits for the {label}"),
                span,
            ));
        }
        value = value * 10 + u32::from(byte - b'0');
    }
    *pos = start + count;
    Ok(value)
}

/// Reads the year, which is one or more digits and may carry a sign.
///
/// Four digits is ISO-8601's ordinary form; one to three are accepted because
/// the old `time.unix` went through chrono's `%Y` parser, which reads them, and
/// old callers must keep their answers. More than four are accepted because
/// `time.format` and `to_text` write the shared window's last instant as
/// chrono's expanded year `+10000`, and `time.unix` must read back what those
/// builtins write. The old `%Y` parser rejected years above 9999; this parser
/// is what widens the window, and `unix_seconds_for` refuses anything beyond it.
fn date_year(text: &str, pos: &mut usize, builtin: &str, span: Span) -> Result<i32> {
    let bytes = text.as_bytes();
    let start = *pos;
    let sign = match bytes.get(*pos) {
        Some(b'-') => {
            *pos += 1;
            -1
        }
        Some(b'+') => {
            *pos += 1;
            1
        }
        _ => 1,
    };
    let digits_start = *pos;
    while matches!(bytes.get(*pos), Some(byte) if byte.is_ascii_digit()) {
        *pos += 1;
    }
    if *pos == digits_start {
        return Err(date_parse_error(
            text,
            digits_start,
            builtin,
            "a year of at least one digit",
            span,
        ));
    }
    let value: i64 = text[digits_start..*pos].parse().map_err(|_| {
        date_parse_error(text, start, builtin, "a year that fits the calendar", span)
    })?;
    let signed = sign * value;
    i32::try_from(signed)
        .map_err(|_| date_parse_error(text, start, builtin, "a year that fits the calendar", span))
}

/// One character that the layout requires, such as the `-` between fields.
fn date_expect(
    text: &str,
    pos: &mut usize,
    wanted: u8,
    builtin: &str,
    expected: &str,
    span: Span,
) -> Result<()> {
    if text.as_bytes().get(*pos) != Some(&wanted) {
        return Err(date_parse_error(text, *pos, builtin, expected, span));
    }
    *pos += 1;
    Ok(())
}

/// Parses the ISO-8601 timestamp forms the `time` module accepts.
///
/// Accepted: `YYYY-MM-DD` (the year is one or more digits), optionally followed
/// by `T` (or a space) and `HH:MM:SS`, an optional fraction of one to nine
/// digits after `.` or `,`, and an optional `Z` or `±HH:MM` offset. A timestamp
/// without a zone is read as UTC, the same stance date arithmetic already has.
/// The offset is applied, so the answer is always the UTC reading of the
/// instant the text named.
///
/// Not accepted: leap seconds (`:60`), a fraction with no digits or more than
/// nine, a `HH:MM` time with no seconds, an offset without minutes, trailing
/// characters, and a date that does not exist (30 February, 29 February in a
/// common year). Every refusal is a `Runtime` error naming `builtin` and the
/// byte position where the expected text stopped matching.
fn parse_iso8601(text: &str, builtin: &str, span: Span) -> Result<NaiveDateTime> {
    let mut pos = 0;
    let year_start = pos;
    let year = date_year(text, &mut pos, builtin, span)?;
    date_expect(text, &mut pos, b'-', builtin, "a '-' after the year", span)?;
    let month_start = pos;
    let month = date_digits(text, &mut pos, 2, "month", builtin, span)?;
    if !(1..=12).contains(&month) {
        return Err(date_parse_error(
            text,
            month_start,
            builtin,
            "a month from 1 to 12",
            span,
        ));
    }
    date_expect(text, &mut pos, b'-', builtin, "a '-' after the month", span)?;
    let day_start = pos;
    let day = date_digits(text, &mut pos, 2, "day", builtin, span)?;
    if !(1..=31).contains(&day) {
        return Err(date_parse_error(
            text,
            day_start,
            builtin,
            "a day from 1 to 31",
            span,
        ));
    }
    let date = NaiveDate::from_ymd_opt(year, month, day).ok_or_else(|| {
        // `from_ymd_opt` also refuses a year outside the calendar's range.
        // Blaming the day for `999999-01-01` named a field that was valid, so
        // the year is checked separately and the refusal points at it.
        if NaiveDate::from_ymd_opt(year, 1, 1).is_none() {
            date_parse_error(
                text,
                year_start,
                builtin,
                "a year the calendar can represent",
                span,
            )
        } else {
            date_parse_error(
                text,
                day_start,
                builtin,
                "a day that exists in that month",
                span,
            )
        }
    })?;
    if pos == text.len() {
        return Ok(date.and_time(NaiveTime::MIN));
    }
    if !matches!(text.as_bytes().get(pos), Some(b'T') | Some(b' ')) {
        return Err(date_parse_error(
            text,
            pos,
            builtin,
            "a 'T' or a space between the date and the time",
            span,
        ));
    }
    pos += 1;
    let hour_start = pos;
    let hour = date_digits(text, &mut pos, 2, "hour", builtin, span)?;
    if hour > 23 {
        return Err(date_parse_error(
            text,
            hour_start,
            builtin,
            "an hour from 0 to 23",
            span,
        ));
    }
    date_expect(text, &mut pos, b':', builtin, "a ':' after the hour", span)?;
    let minute_start = pos;
    let minute = date_digits(text, &mut pos, 2, "minute", builtin, span)?;
    if minute > 59 {
        return Err(date_parse_error(
            text,
            minute_start,
            builtin,
            "a minute from 0 to 59",
            span,
        ));
    }
    date_expect(
        text,
        &mut pos,
        b':',
        builtin,
        "a ':' after the minute",
        span,
    )?;
    let second_start = pos;
    let second = date_digits(text, &mut pos, 2, "second", builtin, span)?;
    if second > 59 {
        return Err(date_parse_error(
            text,
            second_start,
            builtin,
            "a second from 0 to 59 (leap seconds are not accepted)",
            span,
        ));
    }
    let mut nanos = 0u32;
    if matches!(text.as_bytes().get(pos), Some(b'.') | Some(b',')) {
        pos += 1;
        let fraction_start = pos;
        while matches!(text.as_bytes().get(pos), Some(byte) if byte.is_ascii_digit()) {
            pos += 1;
        }
        let digits = pos - fraction_start;
        if digits == 0 {
            return Err(date_parse_error(
                text,
                pos,
                builtin,
                "one to nine digits after the decimal mark",
                span,
            ));
        }
        if digits > 9 {
            return Err(date_parse_error(
                text,
                fraction_start,
                builtin,
                "at most nine digits after the decimal mark",
                span,
            ));
        }
        let fraction: u32 = text[fraction_start..pos].parse().map_err(|_| {
            date_parse_error(
                text,
                fraction_start,
                builtin,
                "a whole number of fractional-second digits",
                span,
            )
        })?;
        nanos = fraction * 10u32.pow(9 - digits as u32);
    }
    let mut offset_seconds: i64 = 0;
    match text.as_bytes().get(pos) {
        None => {}
        Some(b'Z') => pos += 1,
        Some(byte @ (b'+' | b'-')) => {
            let sign = if *byte == b'-' { -1 } else { 1 };
            pos += 1;
            let offset_hour_start = pos;
            let offset_hour = date_digits(text, &mut pos, 2, "offset hour", builtin, span)?;
            if offset_hour > 23 {
                return Err(date_parse_error(
                    text,
                    offset_hour_start,
                    builtin,
                    "an offset hour from 0 to 23",
                    span,
                ));
            }
            date_expect(
                text,
                &mut pos,
                b':',
                builtin,
                "a ':' between the offset's hours and minutes",
                span,
            )?;
            let offset_minute_start = pos;
            let offset_minute = date_digits(text, &mut pos, 2, "offset minute", builtin, span)?;
            if offset_minute > 59 {
                return Err(date_parse_error(
                    text,
                    offset_minute_start,
                    builtin,
                    "an offset minute from 0 to 59",
                    span,
                ));
            }
            offset_seconds = sign * i64::from(offset_hour * 3600 + offset_minute * 60);
        }
        Some(_) => {
            return Err(date_parse_error(
                text,
                pos,
                builtin,
                "a 'Z' or a ±HH:MM offset",
                span,
            ))
        }
    }
    if pos != text.len() {
        return Err(date_parse_error(
            text,
            pos,
            builtin,
            "the end of the timestamp",
            span,
        ));
    }
    let time = NaiveTime::from_hms_nano_opt(hour, minute, second, nanos)
        .ok_or_else(|| date_parse_error(text, hour_start, builtin, "a real clock time", span))?;
    let local = date.and_time(time);
    let offset = TimeDelta::try_seconds(offset_seconds)
        .ok_or_else(|| date_parse_error(text, hour_start, builtin, "a real offset", span))?;
    local.checked_sub_signed(offset).ok_or_else(|| {
        date_parse_error(
            text,
            hour_start,
            builtin,
            "a timestamp the calendar can represent after applying the offset",
            span,
        )
    })
}

/// Reads the fixed layout `time.unix` accepted before the shared ISO parser.
///
/// `chrono`'s `%Y-%m-%d %H:%M:%S` reads one- or two-digit month, day, hour,
/// minute and second fields and accepts the leap second `:60`. The ISO parser
/// is deliberately stricter, so `time.unix` falls back to this layout after it
/// refuses, keeping the answers old callers already had. `time.parse` does not
/// use this fallback: its one-argument surface is the ISO forms only.
fn parse_legacy_unix_text(text: &str) -> Option<NaiveDateTime> {
    chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S").ok()
}

/// Reads `text` with a caller-chosen layout, the shape another system emits.
///
/// The layout is a `chrono` format string, the same language `time.format`
/// writes. A layout with a zone (`%z`, `%:z`, `%Z`, `%+`) yields the UTC
/// reading of the instant; a zone-free layout is read as UTC, the stance every
/// date value has. A date-only layout answers midnight.
///
/// The three parsers are tried in order because `chrono` splits the job: only
/// `DateTime` reads an offset, only `NaiveDateTime` reads a clock time without
/// one, and only `NaiveDate` reads a date on its own. A layout that cannot be
/// read at all is refused before any parsing, rather than handing a malformed
/// layout to a parser whose failure would not say why.
fn parse_with_layout(text: &str, format: &str, builtin: &str, span: Span) -> Result<NaiveDateTime> {
    if StrftimeItems::new(format).any(|item| matches!(item, Item::Error)) {
        return Err(Error::Runtime(
            format!("{builtin} does not understand the format {format:?}"),
            span,
        ));
    }
    if let Ok(instant) = chrono::DateTime::parse_from_str(text, format) {
        return Ok(instant.naive_utc());
    }
    if let Ok(date) = NaiveDateTime::parse_from_str(text, format) {
        return Ok(date);
    }
    if let Ok(date) = NaiveDate::parse_from_str(text, format) {
        return Ok(date.and_time(NaiveTime::MIN));
    }
    Err(Error::Runtime(
        format!("{builtin} could not read {text:?} with the layout {format:?}"),
        span,
    ))
}

/// Formats a date with a caller-chosen layout, refusing a layout `chrono`
/// cannot read rather than letting its display panic.
///
/// `Item::Error` catches text that is not a layout at all. Some layouts parse
/// but cannot format — `%#z` is the parse-only permissive offset — and those
/// fail inside chrono's writer with `fmt::Error`, which `to_string` would turn
/// into a panic. Writing into a `String` keeps that failure an ordinary
/// `Result` the caller can catch.
///
/// The instant is a UTC `DateTime`, not a naive one, so a layout that renders a
/// zone (`%Z`, `%z`, `%+`) names UTC instead of rendering nothing. Date values
/// are read as UTC before they reach here.
fn formatted_date(
    date: chrono::DateTime<chrono::Utc>,
    format: &str,
    builtin: &str,
    span: Span,
) -> Result<Value> {
    if StrftimeItems::new(format).any(|item| matches!(item, Item::Error)) {
        return Err(Error::Runtime(
            format!("{builtin} does not understand the format {format:?}"),
            span,
        ));
    }
    let mut rendered = String::new();
    write!(rendered, "{}", date.format(format)).map_err(|_| {
        Error::Runtime(
            format!("{builtin} does not understand the format {format:?}"),
            span,
        )
    })?;
    Ok(Value::Text(rendered))
}

/// The epoch seconds `time.unix` reports for a date value.
///
/// The same window `seconds_for_format` uses, so the two agree on both sides. A
/// date outside it is a catchable `Runtime` error naming `time.unix` rather
/// than a number no other builtin will take.
///
/// The window ends exactly at `10000-01-01T00:00:00Z`, so a fraction in that
/// final second is past the end even though the whole second it truncates to
/// is still the boundary. Checking only the whole second let
/// `+10000-01-01T00:00:00.5Z` through, and the number it produced was one
/// `time.format` refuses. The fraction is part of the instant, so it is part
/// of the bound.
fn unix_seconds_for(date: NaiveDateTime, builtin: &str, span: Span) -> Result<i64> {
    let seconds = date.and_utc().timestamp();
    const MAX_SECONDS: i64 = 253_402_300_800; // 10000-01-01T00:00:00Z
    let past_the_end = seconds > MAX_SECONDS || (seconds == MAX_SECONDS && date.nanosecond() > 0);
    if seconds < -MAX_SECONDS || past_the_end {
        return Err(Error::Runtime(
            format!(
                "{builtin} cannot represent {} as seconds from 1970-01-01T00:00:00Z",
                crate::value::date_text(date)
            ),
            span,
        ));
    }
    Ok(seconds)
}

/// How long `time.sleep` should wait, given the number the program passed.
///
/// `Duration::from_secs_f64` panics on a negative, NaN, infinite, or
/// overflowing argument, and it does so from inside `core`, past every `Result`
/// in this crate: `time.sleep(-1)` unwound the interpreter thread and replaced
/// the program's failure with "The interpreter thread stopped unexpectedly", so
/// a Redblue `try`/`catch error` could not catch it. That is the same defect
/// class this phase exists to remove, one function away.
///
/// `Duration::try_from_secs_f64` is the same conversion reported as a `Result`,
/// so the refusal is ordinary. The upper bound is not a guess: `try_from_secs_f64`
/// rejects past `u64::MAX` nanoseconds' worth of seconds, and the bound below is
/// strictly inside what it accepts, so the sleep always reaches `thread::sleep`
/// as a real `Duration`.
///
/// A fractional sleep keeps the behaviour it always had — `time.sleep(0.25)`
/// waits 250ms — so this refuses only what would panic, and says which of the
/// three reasons it is refusing.
/// Carries out a sleep [`sleep_duration`] already agreed to.
///
/// `std::thread::sleep` panics on `wasm32-unknown-unknown` — there is no thread
/// to put it on — and a panic there aborts the whole module instance instead of
/// failing one run, so a playground whose `time.sleep` took the page down with
/// it would be worse than one that says it cannot. Blocking the module's only
/// thread would freeze the page besides. So the playground refuses, and says so:
/// the *request* was valid, and `sleep_duration` has already refused the
/// arguments no sleep could honour. The wait is the whole of what this cannot
/// do; nothing about how a program uses the time it waited is affected, because
/// no program gets past this line.
#[cfg(not(target_arch = "wasm32"))]
fn sleep_for(duration: Duration, _span: Span) -> Result<()> {
    std::thread::sleep(duration);
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn sleep_for(duration: Duration, span: Span) -> Result<()> {
    Err(Error::Runtime(
        format!(
            "time.sleep cannot wait in the WebAssembly playground: the module has one thread \
             and blocking it would freeze the page. The request for {duration:?} was valid; \
             the playground cannot carry it out."
        ),
        span,
    ))
}

fn sleep_duration(seconds: f64, span: Span) -> Result<Duration> {
    if !seconds.is_finite() {
        return Err(Error::Runtime(
            format!("time.sleep requires a finite number of seconds, got {seconds}"),
            span,
        ));
    }
    if seconds < 0.0 {
        return Err(Error::Runtime(
            format!("time.sleep requires a number of seconds that is not negative, got {seconds}"),
            span,
        ));
    }
    const MAX_SLEEP_SECONDS: f64 = 31_536_000.0; // one year
    if seconds > MAX_SLEEP_SECONDS {
        return Err(Error::Runtime(
            format!("time.sleep cannot wait {seconds} seconds, at most {MAX_SLEEP_SECONDS}"),
            span,
        ));
    }
    Duration::try_from_secs_f64(seconds).map_err(|_| {
        Error::Runtime(
            format!("time.sleep requires a number of seconds it can wait, got {seconds}"),
            span,
        )
    })
}

/// A whole-number argument to a date builtin, refusing a fraction or a value
/// that is not a finite number.
fn whole_argument(
    args: &[Value],
    index: usize,
    builtin: &str,
    component: &str,
    span: Span,
) -> Result<i64> {
    let Some(Value::Number(n)) = args.get(index) else {
        return Err(Error::Runtime(
            format!("{builtin} requires a whole number for {component}"),
            span,
        ));
    };
    if !n.is_finite() || n.fract() != 0.0 {
        return Err(Error::Runtime(
            format!("{builtin} requires a whole number for {component}, got {n}"),
            span,
        ));
    }
    if *n < i64::MIN as f64 || *n > i64::MAX as f64 {
        return Err(Error::Runtime(
            format!("{builtin} cannot represent {n} as {component}"),
            span,
        ));
    }
    Ok(*n as i64)
}

/// `time.date` and `time.datetime`: six components, or three, as a naive date.
fn date_from_arguments(
    args: &[Value],
    builtin: &str,
    with_time: bool,
    span: Span,
) -> Result<NaiveDateTime> {
    let expected = if with_time { 6 } else { 3 };
    if args.len() != expected {
        return Err(Error::Runtime(
            format!("{builtin} requires {expected} numbers"),
            span,
        ));
    }
    let year = i32::try_from(whole_argument(args, 0, builtin, "year", span)?)
        .map_err(|_| Error::Runtime(format!("{builtin} cannot represent that year"), span))?;
    let month = u32::try_from(whole_argument(args, 1, builtin, "month", span)?)
        .map_err(|_| Error::Runtime(format!("{builtin} cannot represent that month"), span))?;
    let day = u32::try_from(whole_argument(args, 2, builtin, "day", span)?)
        .map_err(|_| Error::Runtime(format!("{builtin} cannot represent that day"), span))?;
    let date = NaiveDate::from_ymd_opt(year, month, day).ok_or_else(|| {
        Error::Runtime(
            format!("{builtin} was given {year}-{month:02}-{day:02}, which is not a real date"),
            span,
        )
    })?;
    if !with_time {
        return Ok(date.and_time(NaiveTime::MIN));
    }
    let hour = whole_argument(args, 3, builtin, "hour", span)?;
    let minute = whole_argument(args, 4, builtin, "minute", span)?;
    let second = whole_argument(args, 5, builtin, "second", span)?;
    let hour = u32::try_from(hour)
        .map_err(|_| Error::Runtime(format!("{builtin} cannot represent that hour"), span))?;
    let minute = u32::try_from(minute)
        .map_err(|_| Error::Runtime(format!("{builtin} cannot represent that minute"), span))?;
    let second = u32::try_from(second)
        .map_err(|_| Error::Runtime(format!("{builtin} cannot represent that second"), span))?;
    date.and_hms_opt(hour, minute, second).ok_or_else(|| {
        Error::Runtime(
            format!(
                "{builtin} was given {year}-{month:02}-{day:02} \
                 {hour:02}:{minute:02}:{second:02}, which is not a real time"
            ),
            span,
        )
    })
}

/// The date a date-arithmetic builtin was given, refusing any other kind.
fn expect_date(value: Option<&Value>, builtin: &str, span: Span) -> Result<NaiveDateTime> {
    match value {
        Some(Value::Date(date)) => Ok(*date),
        Some(other) => Err(Error::Runtime(
            format!(
                "{builtin} requires a date, but was given a {}",
                other.type_name()
            ),
            span,
        )),
        None => Err(Error::Runtime(format!("{builtin} requires a date"), span)),
    }
}

/// `time.duration`: a signed number of seconds, with millisecond precision.
fn duration_from_arguments(args: &[Value], span: Span) -> Result<TimeDelta> {
    let Some(Value::Number(seconds)) = args.first() else {
        return Err(Error::Runtime(
            "time.duration requires a number of seconds".to_string(),
            span,
        ));
    };
    if !seconds.is_finite() {
        return Err(Error::Runtime(
            format!("time.duration requires a finite number of seconds, got {seconds}"),
            span,
        ));
    }
    let milliseconds = (seconds * 1000.0).round();
    if milliseconds < i64::MIN as f64 || milliseconds > i64::MAX as f64 {
        return Err(Error::Runtime(
            format!("time.duration cannot represent {seconds} seconds"),
            span,
        ));
    }
    TimeDelta::try_milliseconds(milliseconds as i64).ok_or_else(|| {
        Error::Runtime(
            format!("time.duration cannot represent {seconds} seconds"),
            span,
        )
    })
}

/// `time.add_years` and `time.add_months`: calendar arithmetic that clamps a
/// day past the end of the target month.
fn add_calendar(
    span: Span,
    date: NaiveDateTime,
    amount: i64,
    months_per_unit: i64,
    builtin: &str,
) -> Result<Value> {
    let months = amount
        .checked_mul(months_per_unit)
        .ok_or_else(|| Error::Runtime(format!("{builtin} cannot move the date that far"), span))?;
    let magnitude = months.unsigned_abs();
    if magnitude > i32::MAX as u64 {
        return Err(Error::Runtime(
            format!("{builtin} cannot move the date that far"),
            span,
        ));
    }
    let moved = if months >= 0 {
        date.checked_add_months(Months::new(magnitude as u32))
    } else {
        date.checked_sub_months(Months::new(magnitude as u32))
    };
    moved
        .map(Value::Date)
        .ok_or_else(|| Error::Runtime(format!("{builtin} cannot move the date that far"), span))
}

/// `time.add_days` and friends: a fixed span, with no daylight-saving rule.
fn add_fixed(
    span: Span,
    date: NaiveDateTime,
    delta: Option<TimeDelta>,
    builtin: &str,
) -> Result<Value> {
    let delta = delta
        .ok_or_else(|| Error::Runtime(format!("{builtin} cannot move the date that far"), span))?;
    date.checked_add_signed(delta)
        .map(Value::Date)
        .ok_or_else(|| Error::Runtime(format!("{builtin} cannot move the date that far"), span))
}

// ============================================================ the seeded draws

// The state the `random*` builtins draw from, per thread.
//
// It used to be `SystemTime::now()`. A clock is the worst possible source for
// this: it is monotonically non-decreasing, so consecutive draws come out
// strictly increasing rather than spread over the range, `random(1, 1)`
// answered whatever the low digits of the instant were because it took no
// arguments at all, and nothing a program printed was reproducible — which is
// AGENTS.md 3.1.4 broken, and a precondition of the bootstrap fixed point with
// it.
//
// `random_seed(n)` sets this. A program that never seeds gets a fixed
// `DEFAULT_SEED` rather than a clock reading, so the language is deterministic
// by default and a program has to *ask* for variation it cannot reproduce.
// That is the direction the failure should be hard to get into.
//
// `Cell<u64>` rather than an `AtomicU64` or a mutex: the state is thread-local,
// so `cargo test` running several tests in parallel cannot interleave two
// tests' draws and make a pinned sequence unreproducible, and no draw pays for
// a lock. A Redblue program is single-threaded — `crate::vm` runs one — so
// thread-local *is* process-local here.
thread_local! {
    static RANDOM_STATE: Cell<u64> = const { Cell::new(DEFAULT_SEED) };
}

/// The seed a program that never calls `random_seed` draws from.
///
/// Fixed, not derived from anything: the default has to be as reproducible as an
/// explicit seed, or the determinism above is only for programs that remember to
/// ask for it. The value is arbitrary — nothing depends on it being this one,
/// and no golden output is pinned to it.
const DEFAULT_SEED: u64 = 0x9E37_79B9_7F4A_7C15;

/// The mixing function of SplitMix64: a bijection on `u64` with every input bit
/// reaching every output bit.
///
/// This is the whole generator. A linear congruential step would also be short,
/// but its low bits are the high bits of the previous step and the low bits are
/// what `random(0, 100)` selects on, so an LCG modulo 101 walks its draw in a
/// short cycle instead of spreading.
const fn splitmix64(state: u64) -> u64 {
    let mut z = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The next 64 random bits, advancing the generator.
fn next_bits() -> u64 {
    RANDOM_STATE.with(|state| {
        let next = splitmix64(state.get());
        state.set(next);
        next
    })
}

/// A draw in `[0, 1)`, from the top 53 bits — the most an `f64` can name without
/// rounding, so every one of the `2^53` steps is equally likely to produce it.
fn next_unit() -> f64 {
    (next_bits() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
}

/// Seeds this thread's draws, so everything after is reproducible from `seed`.
pub fn seed_random(seed: u64) {
    RANDOM_STATE.with(|state| state.set(splitmix64(seed)));
}

/// A whole-number draw from the inclusive range `[min, max]`.
///
/// Both ends are inclusive because that is what makes `random(5, 5)` answer
/// `5`: a range with one member has one answer, and a half-open range would give
/// `random(5, 5)` a span of zero. The members are the whole numbers in the
/// range, so `[0, 100]` has 101 of them and 200 draws can name at most 101.
///
/// The members are `ceil(min)..=floor(max)`, and every answer is one of them —
/// never a fraction and never a step past the high end. That is what makes the
/// draw whole-valued when a program writes a fractional bound: the members of
/// `[0, 100.5]` are `0..=100`, not `0..=100.5`, so the draw is `100` and never
/// `100.5` or `101`. Scaling the width directly got that wrong, because
/// `0 + floor(u * 101.5)` is `101` when `u` is near 1 — outside the range, and
/// with `min` fractional it is not even a whole number.
///
/// A range with no whole number in it — `[0.2, 0.8]` — has no member to name,
/// so it is refused rather than answered with a fraction this draw does not
/// promise. That is a different question from a reversed range, and both are
/// refused by name.
///
/// `min` above `max` is a range with no member either, and is refused rather
/// than answered from the reversed range: silently swapping the ends would make
/// `random(10, 1)` a working call, which is not what the program wrote.
fn random_int(span: Span, min: f64, max: f64) -> Result<Value> {
    if min > max {
        return Err(Error::Runtime(
            format!("random: the low end {min} is above the high end {max}"),
            span,
        ));
    }
    // `-1e308` to `1e308` is a perfectly ordinary range a program might write
    // and it holds whole numbers; the width check below is about whether the
    // *number of members* is measurable, not about whether the ends are.
    let low = min.ceil();
    let high = max.floor();
    if low > high {
        return Err(Error::Runtime(
            format!(
                "random: the range {min} to {max} has no whole number in it, and this \
                 draw names whole numbers"
            ),
            span,
        ));
    }
    // `high - low + 1` is infinity for a range as wide as `-1e308` to `1e308`, and
    // a draw out of a member count no double can name is not a number this
    // language can hold.
    let width = high - low + 1.0;
    if !width.is_finite() {
        return Err(Error::Runtime(
            "random: the range is too wide to measure in a number".to_string(),
            span,
        ));
    }
    // `2^53` is the largest member count whose members are all exactly
    // representable, so above it the modulo would be biased toward the low end by
    // more than the language can name. Such a range is still drawn from, by
    // scaling, which stays inside `[low, high]` because the width is a whole
    // number of members and the last step is clamped to it.
    let draw = if width <= MAX_EXACT_WIDTH {
        low + (next_bits() % width as u64) as f64
    } else {
        low + (next_unit() * width).floor().min(width - 1.0)
    };
    Value::number(draw, span)
}

/// The widest integer range every member of which is exactly representable as
/// an `f64`, so that modulo over it names members evenly.
const MAX_EXACT_WIDTH: f64 = 9_007_199_254_740_992.0; // 2^53

/// A float draw from `[min, max)`, the range `random_number` has always meant.
///
/// `min` above `max` is refused for the same reason `random` refuses it.
fn random_float(span: Span, min: f64, max: f64) -> Result<Value> {
    if min > max {
        return Err(Error::Runtime(
            format!("random_number: the low end {min} is above the high end {max}"),
            span,
        ));
    }
    // `max - min` overflows for a range as ordinary as `-1e308` to `1e308`,
    // which is a number that does not exist. `min + r * inf` is then infinity,
    // or `NaN` when `r` is zero, and `Value::number` refuses both.
    Value::number(min + next_unit() * (max - min), span)
}

/// The `min` and `max` a range builtin was given.
///
/// A single argument is the high end and zero is the low end, so
/// `random_number(10)` is `[0, 10)` and `random(6)` is `[0, 6]` — the dice
/// spelling. No arguments is the unit range for `random_number` and `[0, 100]`
/// for `random`.
///
/// A non-number is refused rather than read as zero: `random("a", 1)` used to
/// answer a draw out of `[0, 1)` because the argument was ignored, which is a
/// working call that means nothing.
fn random_range(span: Span, name: &str, args: &[Value], default_max: f64) -> Result<(f64, f64)> {
    match (args.first(), args.get(1)) {
        (None, None) => Ok((0.0, default_max)),
        (Some(Value::Number(min)), Some(Value::Number(max))) => Ok((*min, *max)),
        (Some(Value::Number(max)), None) => Ok((0.0, *max)),
        _ => Err(Error::Runtime(
            format!("{name} requires numbers, as in {name}(1, 6)"),
            span,
        )),
    }
}

/// `input`: the prompt goes out, one line comes back.
///
/// The prompt goes through [`crate::wasm::emit`] for the same reason `say` does
/// — a page that can collect the answer's program but not its prompt would show
/// a program reading input with nothing to show for it.
///
/// `ask` is not a second spelling of this. It is a **freed keyword**: the lexer
/// reserved it, no parser arm read it, and phase-049 gave it back as an
/// ordinary identifier — `tests/keyword_reservation_test.rs` pins that a program
/// may write `set ask to 1`. Registering it here would take back a word the
/// language has promised to give away, so `ask "n"` is `Unknown variable 'ask'`
/// and stays that way. `input` is the name; there is no `console.input`.
#[cfg(not(target_arch = "wasm32"))]
fn read_line(args: &[Value], span: Span) -> Result<String> {
    if let Some(Value::Text(prompt)) = args.first() {
        crate::wasm::emit(prompt);
    }
    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .map_err(|e| Error::Runtime(e.to_string(), span))?;
    // Only the line terminator goes, and only when there is one. `read_line`
    // keeps whatever it read, so a pipe that ends without a newline — `printf
    // abc`, or a file whose last line has none — hands back `"abc"`, and
    // `input.pop()` dropped its `c`: the answer was a different text from the
    // one that was typed. A `\r` is dropped only after a `\n`, so a lone `\r`
    // inside the text stays where it is.
    let line = input.strip_suffix('\n').unwrap_or(&input);
    Ok(line.strip_suffix('\r').unwrap_or(line).to_string())
}

/// The playground refuses, the way `network` and `time.sleep` do.
///
/// `wasm32-unknown-unknown` has no stdin, so `std::io::stdin` there cannot
/// return: the read blocks forever, which on the module's only thread is a frozen
/// page rather than a failure a page can display. There is nothing to prompt
/// with either — the program has no user to ask — so the honest answer is the
/// refusal below rather than an empty string that would read as "the user pressed
/// enter".
#[cfg(target_arch = "wasm32")]
fn read_line(_args: &[Value], span: Span) -> Result<String> {
    Err(Error::Runtime(
        "input is not available in the WebAssembly playground: a program there has no terminal \
         to read from. Run it with the native rb binary to read a line of input."
            .to_string(),
        span,
    ))
}

/// The memory behind one `lru_cache`-decorated function.
///
/// Entries are kept oldest-first. A hit moves its entry to the end and a miss
/// inserts at the end, so the front is always the least recently used and is
/// what eviction removes when the size would pass the stated capacity.
///
/// A linear scan rather than a hash map because a key is a `Vec<Value>` and a
/// Redblue value may contain a list, a record or a function; the capacity is
/// small enough that comparison is the right cost. Two calls with equal
/// arguments are one entry, with function values matched by identity rather
/// than by the language's structural equality; see [`cache_key_eq`].
#[derive(Debug, Clone)]
pub struct LruCache {
    capacity: usize,
    entries: Vec<(Vec<Value>, Value)>,
}

impl LruCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            entries: Vec::new(),
        }
    }

    /// The answer recorded for `args`, if one is remembered, made the most
    /// recently used entry by the read.
    pub fn get(&mut self, args: &[Value]) -> Option<Value> {
        let index = self
            .entries
            .iter()
            .position(|(remembered, _)| cache_key_eq(remembered, args))?;
        let entry = self.entries.remove(index);
        let value = entry.1.clone();
        self.entries.push(entry);
        Some(value)
    }

    /// Drops every remembered answer, keeping the capacity for the next run.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Records `value` for `args`, evicting the least recently used entry when
    /// the cache is already at its capacity.
    pub fn insert(&mut self, args: &[Value], value: Value) {
        if self.capacity == 0 {
            return;
        }
        if let Some(index) = self
            .entries
            .iter()
            .position(|(remembered, _)| cache_key_eq(remembered, args))
        {
            self.entries.remove(index);
        }
        while self.entries.len() >= self.capacity {
            self.entries.remove(0);
        }
        self.entries.push((args.to_vec(), value));
    }
}

/// Whether two argument lists are the same `lru_cache` key.
///
/// The language's `is` compares two function values by name, parameters and
/// captured bindings, because two closures from one declaration over equal
/// environments are interchangeable. A cache key cannot use that equality: two
/// different declarations can produce functions with the same name, parameters
/// and captures but different bodies, and one must never answer from the
/// other's entry. A function in a key is therefore matched by its identity, and
/// the rule reaches through lists and records because a function can be an
/// item or a field. Every other kind of value keeps the language's equality.
fn cache_key_eq(left: &[Value], right: &[Value]) -> bool {
    left.len() == right.len() && left.iter().zip(right).all(|(a, b)| cache_value_eq(a, b))
}

fn cache_value_eq(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Function(a), Value::Function(b)) => a.id == b.id,
        (Value::List(a), Value::List(b)) => cache_key_eq(a, b),
        (Value::Record(a), Value::Record(b)) => {
            a.len() == b.len()
                && a.iter().all(|(name, value)| {
                    b.get(name)
                        .is_some_and(|other| cache_value_eq(value, other))
                })
        }
        (Value::Object(an, af), Value::Object(bn, bf)) => {
            an == bn
                && af.len() == bf.len()
                && af.iter().all(|(name, value)| {
                    bf.get(name)
                        .is_some_and(|other| cache_value_eq(value, other))
                })
        }
        _ => left == right,
    }
}

/// A `take`/`skip` count as a whole number of items.
///
/// A positive count that is not a whole number is refused rather than silently
/// truncated: there is no such thing as "the first 1.9 items", and rounding
/// would make the caller's arithmetic disappear without a word. The ends are
/// clamped rather than refused: a negative count is zero and a count past the
/// end is the whole list, which is what makes `take(xs, 0)` empty and
/// `skip(xs, 999)` empty without either being an error. A count that is not a
/// finite number is refused, because `NaN` and an infinity have no place in
/// "the first n items".
fn iteration_count(name: &str, count: f64, span: Span) -> Result<usize> {
    if !count.is_finite() {
        return Err(Error::Runtime(
            format!("{name} requires a finite number of items, got {count}"),
            span,
        ));
    }
    if count <= 0.0 {
        return Ok(0);
    }
    if count.fract() != 0.0 {
        return Err(Error::Runtime(
            format!("{name} requires a whole number of items, got {count}"),
            span,
        ));
    }
    Ok(count.trunc() as usize)
}

/// The most pairs one `product` call may materialise.
///
/// A builtin call is charged as one step, so the step budget cannot stop a
/// list whose length is the product of its inputs. Two lists of 100,000 items
/// are each buildable under the loop cap, but their product is ten billion
/// pairs — enough to exhaust the host before any guard can run. This bound is
/// the one that call checks before it reserves anything, and exceeding it is a
/// [`Error::Limit`], not a panic.
pub const MAX_PRODUCT_ITEMS: usize = 1_000_000;

const HASH_NOTHING: u8 = 1;
const HASH_NUMBER: u8 = 2;
const HASH_TEXT: u8 = 3;
const HASH_YES_NO: u8 = 4;
const HASH_LIST: u8 = 5;
const HASH_RECORD: u8 = 6;
const HASH_OBJECT_NAME: u8 = 7;
const HASH_OBJECT_FIELDS: u8 = 8;
const HASH_FUNCTION_NAME: u8 = 9;
const HASH_FUNCTION_PARAMS_LEN: u8 = 10;
const HASH_FUNCTION_PARAM: u8 = 11;
const HASH_FUNCTION_SCOPES_LEN: u8 = 12;
const HASH_FIELD_NAME: u8 = 13;
const HASH_BUILTIN: u8 = 14;
const HASH_DATE: u8 = 15;
const HASH_DURATION: u8 = 16;

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

fn hash_tagged_bytes(tag: u8, bytes: &[u8]) -> u64 {
    let mut hash = FNV_OFFSET ^ u64::from(tag);
    hash = hash.wrapping_mul(FNV_PRIME);
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

fn hash_tagged_u64(tag: u8, value: u64) -> u64 {
    hash_tagged_bytes(tag, &value.to_le_bytes())
}

fn combine_hash(hash: u64, value: u64) -> u64 {
    (hash ^ value).wrapping_mul(FNV_PRIME)
}

/// A deterministic hash whose only invariant is that `a == b` implies
/// `hash(a) == hash(b)`.
///
/// `groupby` uses it to find candidate groups without comparing every item
/// with every earlier key. Equality is still [`Value`]'s own, so a collision
/// costs one comparison rather than changing the answer. Record and object
/// fields are combined commutatively because their `IndexMap` equality is
/// order-independent; captured scopes are combined in order because a
/// function's captured stack is.
fn group_value_hash(value: &Value) -> u64 {
    match value {
        Value::Nothing => hash_tagged_u64(HASH_NOTHING, 0),
        Value::Number(number) => {
            let bits = if *number == 0.0 {
                0.0f64.to_bits()
            } else {
                number.to_bits()
            };
            hash_tagged_u64(HASH_NUMBER, bits)
        }
        Value::Text(text) => hash_tagged_bytes(HASH_TEXT, text.as_bytes()),
        Value::YesNo(flag) => hash_tagged_u64(HASH_YES_NO, u64::from(*flag)),
        Value::List(items) => {
            let mut hash = hash_tagged_u64(HASH_LIST, items.len() as u64);
            for item in items.iter() {
                hash = combine_hash(hash, group_value_hash(item));
            }
            hash
        }
        Value::Record(fields) => group_fields_hash(HASH_RECORD, fields),
        Value::Object(name, fields) => {
            let mut hash = hash_tagged_bytes(HASH_OBJECT_NAME, name.as_bytes());
            hash = combine_hash(hash, group_fields_hash(HASH_OBJECT_FIELDS, fields));
            hash
        }
        Value::Function(function) => group_function_hash(function),
        Value::Builtin(name) => hash_tagged_bytes(HASH_BUILTIN, name.as_bytes()),
        Value::Date(date) => hash_tagged_bytes(HASH_DATE, format!("{:?}", date).as_bytes()),
        Value::Duration(duration) => {
            hash_tagged_bytes(HASH_DURATION, format!("{:?}", duration).as_bytes())
        }
    }
}

fn group_fields_hash(tag: u8, fields: &crate::value::Fields) -> u64 {
    let hash = hash_tagged_u64(tag, fields.len() as u64);
    let mut entries = 0u64;
    for (name, value) in fields.iter() {
        let key = hash_tagged_bytes(HASH_FIELD_NAME, name.as_bytes());
        entries = entries.wrapping_add(combine_hash(key, group_value_hash(value)));
    }
    combine_hash(hash, entries)
}

fn group_function_hash(function: &crate::value::FunctionValue) -> u64 {
    let mut hash = hash_tagged_bytes(HASH_FUNCTION_NAME, function.name.as_bytes());
    hash = combine_hash(
        hash,
        hash_tagged_u64(HASH_FUNCTION_PARAMS_LEN, function.params.len() as u64),
    );
    for param in &function.params {
        hash = combine_hash(
            hash,
            hash_tagged_bytes(HASH_FUNCTION_PARAM, param.as_bytes()),
        );
    }
    hash = combine_hash(
        hash,
        hash_tagged_u64(HASH_FUNCTION_SCOPES_LEN, function.captured.len() as u64),
    );
    for scope in function.captured.iter() {
        let mut entries = 0u64;
        for (name, value) in scope.iter() {
            let key = hash_tagged_bytes(HASH_FIELD_NAME, name.as_bytes());
            entries = entries.wrapping_add(combine_hash(key, group_value_hash(value)));
        }
        hash = combine_hash(hash, entries);
    }
    hash
}

/// The `[key, value]` pairs a Counter or a defaultdict stores.
fn collection_pairs(span: Span, value: &Value, what: &str) -> Result<Vec<(Value, Value)>> {
    let Value::List(pairs) = value else {
        return Err(Error::Runtime(
            format!("{what} requires a list of [key, value] pairs"),
            span,
        ));
    };
    let mut out = Vec::with_capacity(pairs.len());
    for pair in pairs.iter() {
        let Value::List(pair) = pair else {
            return Err(Error::Runtime(
                format!("{what} requires [key, value] pairs"),
                span,
            ));
        };
        let [key, value] = pair.as_slice() else {
            return Err(Error::Runtime(
                format!("{what} requires [key, value] pairs"),
                span,
            ));
        };
        out.push((key.clone(), value.clone()));
    }
    Ok(out)
}

fn counter_value(pairs: Vec<(Value, Value)>) -> Value {
    let counts = pairs
        .into_iter()
        .map(|(key, count)| Value::list(vec![key, count]))
        .collect();
    let mut fields = Fields::new();
    fields.insert("counts".to_string(), Value::list(counts));
    fields.insert("missing".to_string(), Value::Number(0.0));
    Value::record(fields)
}

fn counter_parts(span: Span, counter: &Value) -> Result<(Vec<(Value, Value)>, Value)> {
    let Value::Record(fields) = counter else {
        return Err(Error::Runtime(
            "a counter is a record made by collections.counter".to_string(),
            span,
        ));
    };
    let Some(counts) = fields.get("counts") else {
        return Err(Error::Runtime(
            "a counter is a record made by collections.counter".to_string(),
            span,
        ));
    };
    let pairs = collection_pairs(span, counts, "a counter")?;
    for (_, count) in &pairs {
        counter_number(span, count)?;
    }
    let Some(missing) = fields.get("missing") else {
        return Err(Error::Runtime(
            "a counter is a record made by collections.counter".to_string(),
            span,
        ));
    };
    Ok((pairs, missing.clone()))
}

fn counter_number(span: Span, value: &Value) -> Result<f64> {
    match value {
        Value::Number(number) if number.is_finite() && *number >= 0.0 && number.fract() == 0.0 => {
            Ok(*number)
        }
        _ => Err(Error::Runtime(
            "a counter's count must be a non-negative integer".to_string(),
            span,
        )),
    }
}

fn defaultdict_value(pairs: Vec<(Value, Value)>, factory: Value) -> Value {
    let entries = pairs
        .into_iter()
        .map(|(key, value)| Value::list(vec![key, value]))
        .collect();
    let mut fields = Fields::new();
    fields.insert("entries".to_string(), Value::list(entries));
    fields.insert("factory".to_string(), factory);
    Value::record(fields)
}

fn defaultdict_parts(span: Span, dictionary: &Value) -> Result<(Vec<(Value, Value)>, Value)> {
    let Value::Record(fields) = dictionary else {
        return Err(Error::Runtime(
            "a defaultdict is a record made by collections.defaultdict".to_string(),
            span,
        ));
    };
    let Some(entries) = fields.get("entries") else {
        return Err(Error::Runtime(
            "a defaultdict is a record made by collections.defaultdict".to_string(),
            span,
        ));
    };
    let pairs = collection_pairs(span, entries, "a defaultdict")?;
    let Some(factory) = fields.get("factory") else {
        return Err(Error::Runtime(
            "a defaultdict is a record made by collections.defaultdict".to_string(),
            span,
        ));
    };
    Ok((pairs, factory.clone()))
}

fn defaultdict_answer(value: Value, dictionary: Value) -> Value {
    let mut fields = Fields::new();
    fields.insert("value".to_string(), value);
    fields.insert("dict".to_string(), dictionary);
    Value::record(fields)
}

fn deque_error(span: Span) -> Error {
    Error::Runtime("a value made by collections.deque".to_string(), span)
}

/// Refuses a deque that would hold more than [`MAX_DEQUE_ITEMS`].
///
/// The message names both the limit and the length that crossed it, so a
/// constructor or push refusal says what to remove rather than only saying the
/// value is not a deque.
fn deque_budget_error(span: Span, length: usize) -> Error {
    Error::Runtime(
        format!("a deque holds at most {MAX_DEQUE_ITEMS} items, but this one would hold {length}"),
        span,
    )
}

fn deque_value(front: Value, back: Value) -> Value {
    let mut fields = Fields::new();
    fields.insert("kind".to_string(), Value::Text("deque".to_string()));
    fields.insert("front".to_string(), front);
    fields.insert("back".to_string(), back);
    // An `Object` rather than a `Record` because source cannot build one: a
    // deque made here is trusted, while a hand-written record with the same
    // field names is not. Both are accepted by `deque_parts`, but only the
    // trusted object skips the full tree walk on every operation.
    Value::object("deque".to_string(), fields)
}

/// The two stacks of a deque, and whether they came from this module.
///
/// A value made by `collections.deque` is an `Object`, which no Redblue source
/// can construct, so its trees are known good and an operation may trust them.
/// A hand-written record is still accepted for compatibility and for the
/// forged-tree tests, but it is validated all the way down before use: its
/// `size` fields may disagree with its children, and walking every node is what
/// refuses it rather than trusting a `kind` a program can write.
fn deque_parts(span: Span, deque: &Value) -> Result<(Value, Value)> {
    let (front, back, trusted) = match deque {
        Value::Object(name, fields) if name == "deque" => {
            let Some(Value::Text(kind)) = fields.get("kind") else {
                return Err(deque_error(span));
            };
            if kind != "deque" {
                return Err(deque_error(span));
            }
            let Some(front) = fields.get("front") else {
                return Err(deque_error(span));
            };
            let Some(back) = fields.get("back") else {
                return Err(deque_error(span));
            };
            (front, back, true)
        }
        Value::Record(fields) => {
            let Some(Value::Text(kind)) = fields.get("kind") else {
                return Err(deque_error(span));
            };
            if kind != "deque" {
                return Err(deque_error(span));
            }
            let Some(front) = fields.get("front") else {
                return Err(deque_error(span));
            };
            let Some(back) = fields.get("back") else {
                return Err(deque_error(span));
            };
            (front, back, false)
        }
        _ => return Err(deque_error(span)),
    };
    if !matches!(front, Value::List(_)) || !matches!(back, Value::List(_)) {
        return Err(deque_error(span));
    }
    let mut total = 0usize;
    for stack in [front, back] {
        let Value::List(trees) = stack else {
            return Err(deque_error(span));
        };
        for tree in trees.iter() {
            let size = stack_tree_budget(span, tree)?;
            total = total.checked_add(size).ok_or_else(|| deque_error(span))?;
            if total > MAX_DEQUE_ITEMS {
                return Err(deque_budget_error(span, total));
            }
            if !trusted {
                validate_stack_tree(span, tree)?;
            }
        }
    }
    Ok((front.clone(), back.clone()))
}

/// The most logical nodes one deque operation may validate or enumerate.
///
/// A tree node's `size` is the number of logical nodes its root stands for, and
/// a forged tree can reuse one physical child under both branches so that a few
/// records claim an enormous logical tree. Rejecting an over-budget root before
/// walking it keeps a malformed deque from making one operation visit a tree the
/// deque cannot hold.
const MAX_DEQUE_ITEMS: usize = 1_000_000;

/// The size of one node in a deque's persistent stack.
fn stack_tree_size(span: Span, tree: &Value) -> Result<usize> {
    let Value::Record(fields) = tree else {
        return Err(deque_error(span));
    };
    let Some(Value::Number(size)) = fields.get("size") else {
        return Err(deque_error(span));
    };
    if !size.is_finite() || *size < 1.0 || size.fract() != 0.0 || *size > usize::MAX as f64 {
        return Err(deque_error(span));
    }
    Ok(*size as usize)
}

/// A leaf stack node holding `value`.
fn stack_tree(value: Value) -> Value {
    let mut fields = Fields::new();
    fields.insert("size".to_string(), Value::Number(1.0));
    fields.insert("value".to_string(), value);
    Value::record(fields)
}

/// An internal stack node holding `value` and two equal-sized child trees.
fn stack_node(value: Value, left: Value, right: Value, size: usize) -> Value {
    let mut fields = Fields::new();
    fields.insert("size".to_string(), Value::Number(size as f64));
    fields.insert("value".to_string(), value);
    fields.insert("left".to_string(), left);
    fields.insert("right".to_string(), right);
    Value::record(fields)
}

struct StackTreeParts {
    value: Value,
    children: Option<(Value, Value)>,
}

/// One stack node's value and children, refusing a malformed node.
fn stack_tree_node_parts(span: Span, tree: &Value) -> Result<StackTreeParts> {
    let Value::Record(fields) = tree else {
        return Err(deque_error(span));
    };
    let Some(value) = fields.get("value") else {
        return Err(deque_error(span));
    };
    let size = stack_tree_size(span, tree)?;
    match (fields.get("left"), fields.get("right")) {
        (None, None) => {
            if size != 1 {
                return Err(deque_error(span));
            }
            Ok(StackTreeParts {
                value: value.clone(),
                children: None,
            })
        }
        (Some(left), Some(right)) => {
            let left_size = stack_tree_size(span, left)?;
            let right_size = stack_tree_size(span, right)?;
            let expected = left_size
                .checked_add(right_size)
                .and_then(|size| size.checked_add(1))
                .ok_or_else(|| deque_error(span))?;
            if left_size != right_size || size != expected {
                return Err(deque_error(span));
            }
            Ok(StackTreeParts {
                value: value.clone(),
                children: Some((left.clone(), right.clone())),
            })
        }
        _ => Err(deque_error(span)),
    }
}

/// Refuses a tree whose logical size is beyond what one deque may hold.
fn stack_tree_budget(span: Span, tree: &Value) -> Result<usize> {
    let size = stack_tree_size(span, tree)?;
    if size > MAX_DEQUE_ITEMS {
        return Err(deque_budget_error(span, size));
    }
    Ok(size)
}

/// Refuses a tree with any malformed node, not only a malformed root.
///
/// The root's logical size is checked before the walk, and each physical node is
/// visited once, so a forged DAG that shares one child under both branches is
/// either bounded by the budget or refused before the walk starts instead of
/// being walked once per logical occurrence.
fn validate_stack_tree(span: Span, tree: &Value) -> Result<()> {
    stack_tree_budget(span, tree)?;
    let mut seen = HashSet::new();
    let mut pending = vec![tree];
    while let Some(tree) = pending.pop() {
        let Value::Record(fields) = tree else {
            return Err(deque_error(span));
        };
        if !seen.insert(Arc::as_ptr(fields) as usize) {
            continue;
        }
        if fields.get("value").is_none() {
            return Err(deque_error(span));
        }
        let size = stack_tree_size(span, tree)?;
        match (fields.get("left"), fields.get("right")) {
            (None, None) => {
                if size != 1 {
                    return Err(deque_error(span));
                }
            }
            (Some(left), Some(right)) => {
                let left_size = stack_tree_size(span, left)?;
                let right_size = stack_tree_size(span, right)?;
                let expected = left_size
                    .checked_add(right_size)
                    .and_then(|size| size.checked_add(1))
                    .ok_or_else(|| deque_error(span))?;
                if left_size != right_size || size != expected {
                    return Err(deque_error(span));
                }
                pending.push(left);
                pending.push(right);
            }
            _ => return Err(deque_error(span)),
        }
    }
    Ok(())
}

/// The largest perfect tree size (`2^k - 1`) that fits in `items`.
fn largest_perfect_size(items: usize) -> usize {
    let mut size = 1;
    while size * 2 < items {
        size = size * 2 + 1;
    }
    size
}

/// A perfect tree whose pop order is `values`.
///
/// `values` has `2^k - 1` items, so the root is the first to pop and the two
/// equal halves are the left and right subtrees. Building it directly costs one
/// visit per item; pushing the same items one at a time costs a spine copy per
/// push.
fn stack_tree_from_values(values: &[Value]) -> Value {
    if values.len() == 1 {
        return stack_tree(values[0].clone());
    }
    let half = values.len() / 2;
    let left = stack_tree_from_values(&values[1..=half]);
    let right = stack_tree_from_values(&values[half + 1..]);
    stack_node(values[0].clone(), left, right, values.len())
}

/// A stack whose pop order is `values`, built in one pass.
///
/// The forest uses the largest perfect tree that fits at each step, so
/// `collections.deque(list)` is linear in the list's length rather than paying
/// a spine copy for every item. Each tree still validates through the same
/// `validate_stack_tree` a pushed stack does.
fn stack_from_values(values: &[Value]) -> Value {
    let mut trees = Vec::new();
    let mut rest = values;
    while !rest.is_empty() {
        let size = largest_perfect_size(rest.len());
        let (head, tail) = rest.split_at(size);
        trees.push(stack_tree_from_values(head));
        rest = tail;
    }
    Value::list(trees)
}

/// Pushes `value` onto a stack. Two equal-sized top trees merge into one, so the
/// spine holds O(log n) trees and every node has O(log n) depth. Only the top
/// tree of each spine entry is checked here: a module deque's deeper nodes were
/// checked when it was built, and `deque_parts` already walked a hand-written
/// record all the way down before this runs.
fn stack_cons(span: Span, stack: &Value, value: Value) -> Result<Value> {
    let Value::List(trees) = stack else {
        return Err(deque_error(span));
    };
    for tree in trees.iter() {
        stack_tree_node_parts(span, tree)?;
    }
    let mut trees = trees.as_ref().clone();
    if trees.len() >= 2 {
        let first_size = stack_tree_size(span, &trees[0])?;
        let second_size = stack_tree_size(span, &trees[1])?;
        if first_size == second_size {
            let left = trees.remove(0);
            let right = trees.remove(0);
            let size = first_size
                .checked_add(second_size)
                .and_then(|size| size.checked_add(1))
                .ok_or_else(|| deque_error(span))?;
            trees.insert(0, stack_node(value, left, right, size));
            return Ok(Value::list(trees));
        }
    }
    trees.insert(0, stack_tree(value));
    Ok(Value::list(trees))
}

/// Pops the top of a stack, answering `None` when it is empty.
fn stack_uncons(span: Span, stack: &Value) -> Result<Option<(Value, Value)>> {
    let Value::List(trees) = stack else {
        return Err(deque_error(span));
    };
    let Some(first) = trees.first() else {
        return Ok(None);
    };
    let parts = stack_tree_node_parts(span, first)?;
    let rest = trees.iter().skip(1).cloned().collect::<Vec<_>>();
    match parts.children {
        None => Ok(Some((parts.value, Value::list(rest)))),
        Some((left, right)) => {
            let mut next = Vec::with_capacity(rest.len() + 2);
            next.push(left);
            next.push(right);
            next.extend(rest);
            Ok(Some((parts.value, Value::list(next))))
        }
    }
}

/// Every value a stack holds, in the order it would pop them.
fn stack_pop_order(span: Span, stack: &Value) -> Result<Vec<Value>> {
    let Value::List(trees) = stack else {
        return Err(deque_error(span));
    };
    let mut values = Vec::new();
    for tree in trees.iter() {
        let size = stack_tree_budget(span, tree)?;
        let total = values
            .len()
            .checked_add(size)
            .ok_or_else(|| deque_error(span))?;
        if total > MAX_DEQUE_ITEMS {
            return Err(deque_budget_error(span, total));
        }
        let mut pending = vec![tree.clone()];
        while let Some(node) = pending.pop() {
            if values.len() == MAX_DEQUE_ITEMS {
                return Err(deque_budget_error(span, MAX_DEQUE_ITEMS + 1));
            }
            let parts = stack_tree_node_parts(span, &node)?;
            values.push(parts.value);
            if let Some((left, right)) = parts.children {
                pending.push(right);
                pending.push(left);
            }
        }
    }
    Ok(values)
}

/// Splits the one non-empty stack across both ends when the other end empties.
///
/// Moving every item across would make alternating pops at opposite ends
/// reverse the whole remaining deque on every other pop, which is quadratic.
/// Keeping roughly half at each end means those pops share the trees they do
/// not touch, and each remaining item is rebuilt only a constant number of
/// times as the two ends shrink together.
fn deque_rebalance(span: Span, stack: &Value, wants_left: bool) -> Result<(Value, Value)> {
    let mut values = stack_pop_order(span, stack)?;
    if wants_left {
        values.reverse();
    }
    let front_count = if wants_left {
        values.len().div_ceil(2)
    } else {
        values.len() / 2
    };
    let (front_values, back_values) = values.split_at(front_count);
    let front = stack_from_values(front_values);
    let mut back_values = back_values.to_vec();
    back_values.reverse();
    let back = stack_from_values(&back_values);
    Ok((front, back))
}

fn deque_pop(value: Value, deque: Value) -> Value {
    let mut fields = Fields::new();
    fields.insert("value".to_string(), value);
    fields.insert("deque".to_string(), deque);
    Value::record(fields)
}

/// Every function that is not a user-defined one, in the one place both VMs
/// reach for.
///
/// `Ok(None)` says the name is not a builtin, which is how the caller knows to
/// look for a user declaration. This lives here rather than in either VM so
/// that a bytecode `CALL` and a tree-walked call cannot drift apart: there is
/// one implementation and both answer from it.
pub fn builtin(span: Span, name: &str, args: &[Value]) -> Result<Option<Value>> {
    match name {
        "say" => {
            if let Some(arg) = args.first() {
                // Through the one boundary every program's bytes leave by, as
                // `say` the statement does. A `println!` here would put
                // `say("hi")` on stdout while `say "hi"` went to the playground's
                // capture, and the byte-identical promise is exactly that the two
                // spellings of the same thing print the same thing.
                crate::wasm::emit(&format!("{}\n", arg));
                Ok(Some(Value::Nothing))
            } else {
                Err(Error::Runtime("say requires an argument".to_string(), span))
            }
        }
        "length" | "len" => {
            if let Some(Value::List(items)) = args.first().cloned() {
                Ok(Some(Value::Number(items.len() as f64)))
            } else if let Some(Value::Text(s)) = args.first().cloned() {
                // Characters, not bytes. `s.len()` is the length of the
                // `String` as UTF-8, so every text outside ASCII answered two,
                // three or four times its real length: `length("日本")` said 6
                // for two characters and `length("é")` said 2 for one. README
                // promises "Text is fully Unicode", so the one measurement a
                // program can make about its own text was wrong for every
                // non-ASCII string and carried a plausible-looking number.
                //
                // A character here is one Unicode scalar value — what Rust's
                // `char` is — not one user-perceived glyph. `e` + U+0301 is two
                // characters and one glyph, and answering 1 would mean
                // segmenting grapheme clusters, which is a different
                // measurement under a different name. A program that wants the
                // bytes writes `length(bytes.from_text(s))`, which answers 6
                // for `"héllo"` because a byte is a byte.
                //
                // This is the only implementation of `length` over text:
                // `stdlib::builtin_function` had a second arm that no caller
                // reached, and it is deleted. `tests/text_length_characters_test.rs`
                // pins that there is one.
                Ok(Some(Value::Number(s.chars().count() as f64)))
            } else {
                Err(Error::Runtime(
                    "length requires a list or text".to_string(),
                    span,
                ))
            }
        }
        // -- iteration helpers -------------------------------------------------
        //
        // Every one of these returns a new list rather than an iterator.
        // Redblue has no iterator value, so a helper that wanted to be lazy
        // would have nothing to hand back; SPEC.md § Iteration helpers states
        // that for the reader rather than leaving it to be discovered.
        //
        // `chain` takes any number of lists, so joining three or more needs no
        // nesting and joining none is the empty list.
        "chain" => {
            let mut items = Vec::new();
            for arg in args {
                match arg {
                    Value::List(list) => items.extend(list.iter().cloned()),
                    _ => {
                        return Err(Error::Runtime("chain requires lists".to_string(), span));
                    }
                }
            }
            Ok(Some(Value::list(items)))
        }
        "zip" => {
            let mut lists: Vec<&[Value]> = Vec::with_capacity(args.len());
            for arg in args {
                match arg {
                    Value::List(items) => lists.push(items),
                    _ => {
                        return Err(Error::Runtime(
                            "zip requires at least two lists".to_string(),
                            span,
                        ))
                    }
                }
            }
            if lists.len() < 2 {
                return Err(Error::Runtime(
                    "zip requires at least two lists".to_string(),
                    span,
                ));
            }
            // The answer is as long as the shortest input, so a row never
            // reaches past any list it draws from.
            let shortest = lists.iter().map(|list| list.len()).min().unwrap_or(0);
            let rows = (0..shortest)
                .map(|index| Value::list(lists.iter().map(|list| list[index].clone()).collect()))
                .collect();
            Ok(Some(Value::list(rows)))
        }
        "enumerate" => match args {
            [Value::List(items)] => {
                let indexed = items
                    .iter()
                    .enumerate()
                    .map(|(index, item)| {
                        Value::list(vec![Value::Number(index as f64), item.clone()])
                    })
                    .collect();
                Ok(Some(Value::list(indexed)))
            }
            _ => Err(Error::Runtime(
                "enumerate requires one list".to_string(),
                span,
            )),
        },
        "product" => match args {
            [Value::List(left), Value::List(right)] => {
                let count = match left.len().checked_mul(right.len()) {
                    Some(count) if count <= MAX_PRODUCT_ITEMS => count,
                    _ => {
                        return Err(Error::Limit(
                            format!(
                                "product materialises at most {MAX_PRODUCT_ITEMS} pairs; \
                                 {} items times {} items is past that limit",
                                left.len(),
                                right.len()
                            ),
                            span,
                        ))
                    }
                };
                let mut pairs = Vec::with_capacity(count);
                for a in left.iter() {
                    for b in right.iter() {
                        pairs.push(Value::list(vec![a.clone(), b.clone()]));
                    }
                }
                Ok(Some(Value::list(pairs)))
            }
            _ => Err(Error::Runtime(
                "product requires two lists".to_string(),
                span,
            )),
        },
        // Groups are kept in the order their key first appeared, and a key is
        // matched with `Value`'s equality, so two equal records or lists share
        // one group rather than two. A group is a record — `key` and `items` —
        // so a caller can reach either half by name. The index maps a hash to
        // the groups that share it, so grouping is linear rather than comparing
        // each item with every earlier key; a collision is resolved by the same
        // equality a linear scan used.
        "groupby" => match args {
            [Value::List(items)] => {
                let mut groups: Vec<(Value, Vec<Value>)> = Vec::new();
                let mut index: HashMap<u64, Vec<usize>> = HashMap::new();
                for item in items.iter() {
                    let hash = group_value_hash(item);
                    let existing = index.get(&hash).and_then(|candidates| {
                        candidates
                            .iter()
                            .copied()
                            .find(|&candidate| groups[candidate].0 == *item)
                    });
                    match existing {
                        Some(candidate) => groups[candidate].1.push(item.clone()),
                        None => {
                            let candidate = groups.len();
                            index.entry(hash).or_default().push(candidate);
                            groups.push((item.clone(), vec![item.clone()]));
                        }
                    }
                }
                let records = groups
                    .into_iter()
                    .map(|(key, items)| {
                        let mut fields = crate::value::Fields::new();
                        fields.insert("key".to_string(), key);
                        fields.insert("items".to_string(), Value::list(items));
                        Value::record(fields)
                    })
                    .collect();
                Ok(Some(Value::list(records)))
            }
            _ => Err(Error::Runtime(
                "groupby requires one list".to_string(),
                span,
            )),
        },
        "take" => match args {
            [Value::List(items), Value::Number(count)] => {
                let count = iteration_count("take", *count, span)?;
                Ok(Some(Value::list(
                    items.iter().take(count).cloned().collect(),
                )))
            }
            _ => Err(Error::Runtime(
                "take requires a list and a number".to_string(),
                span,
            )),
        },
        "skip" => match args {
            [Value::List(items), Value::Number(count)] => {
                let count = iteration_count("skip", *count, span)?;
                Ok(Some(Value::list(
                    items.iter().skip(count).cloned().collect(),
                )))
            }
            _ => Err(Error::Runtime(
                "skip requires a list and a number".to_string(),
                span,
            )),
        },
        // -- collections -------------------------------------------------------
        //
        // A Counter is a record `{counts: [[key, count], ...], missing: 0}`.
        // The pairs are in first-appearance order, so `most_common` can sort
        // them stably by count and a tie keeps the order its key first appeared
        // in. The missing value is written into the record rather than assumed,
        // so `collections.count` answers the record's own documented default.
        "collections_counter" => match args {
            [Value::List(items)] => {
                let mut counts: Vec<(Value, f64)> = Vec::new();
                let mut index: HashMap<u64, Vec<usize>> = HashMap::new();
                for item in items.iter() {
                    let hash = group_value_hash(item);
                    let existing = index.get(&hash).and_then(|candidates| {
                        candidates
                            .iter()
                            .copied()
                            .find(|&candidate| counts[candidate].0 == *item)
                    });
                    match existing {
                        Some(candidate) => counts[candidate].1 += 1.0,
                        None => {
                            let candidate = counts.len();
                            index.entry(hash).or_default().push(candidate);
                            counts.push((item.clone(), 1.0));
                        }
                    }
                }
                let pairs = counts
                    .into_iter()
                    .map(|(key, count)| (key, Value::Number(count)))
                    .collect();
                Ok(Some(counter_value(pairs)))
            }
            _ => Err(Error::Runtime(
                "collections.counter requires one list".to_string(),
                span,
            )),
        },
        "collections_count" => match args {
            [counter, key] => {
                let (pairs, missing) = counter_parts(span, counter)?;
                let found = pairs
                    .into_iter()
                    .find(|(candidate, _)| candidate == key)
                    .map(|(_, count)| count);
                Ok(Some(found.unwrap_or(missing)))
            }
            _ => Err(Error::Runtime(
                "collections.count requires a counter and a key".to_string(),
                span,
            )),
        },
        "collections_most_common" => match args {
            [counter] => {
                let (pairs, _) = counter_parts(span, counter)?;
                let mut ordered: Vec<(Value, f64)> = pairs
                    .into_iter()
                    .map(|(key, count)| counter_number(span, &count).map(|count| (key, count)))
                    .collect::<Result<_>>()?;
                // A stable sort, so equal counts keep the order their keys first
                // appeared in the counter rather than an order that depends on
                // the sort implementation.
                ordered.sort_by(|left, right| {
                    right
                        .1
                        .partial_cmp(&left.1)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                let answer = ordered
                    .into_iter()
                    .map(|(key, count)| Value::list(vec![key, Value::Number(count)]))
                    .collect();
                Ok(Some(Value::list(answer)))
            }
            _ => Err(Error::Runtime(
                "collections.most_common requires one counter".to_string(),
                span,
            )),
        },
        // A defaultdict is a record `{entries: [[key, value], ...], factory}`.
        // Redblue values are copied, so the factory is a *value*: every entry
        // the first read creates is a copy of it. That is written down in
        // SPEC.md rather than left to be discovered.
        "collections_defaultdict" => match args {
            [factory] => {
                let mut fields = Fields::new();
                fields.insert("entries".to_string(), Value::list(Vec::new()));
                fields.insert("factory".to_string(), factory.clone());
                Ok(Some(Value::record(fields)))
            }
            _ => Err(Error::Runtime(
                "collections.defaultdict requires one factory value".to_string(),
                span,
            )),
        },
        "collections_get" => match args {
            [dictionary, key] => {
                let (pairs, factory) = defaultdict_parts(span, dictionary)?;
                if let Some((_, value)) = pairs.iter().find(|(candidate, _)| candidate == key) {
                    return Ok(Some(defaultdict_answer(value.clone(), dictionary.clone())));
                }
                let value = factory.clone();
                let mut updated = pairs;
                updated.push((key.clone(), value.clone()));
                Ok(Some(defaultdict_answer(
                    value,
                    defaultdict_value(updated, factory),
                )))
            }
            _ => Err(Error::Runtime(
                "collections.get requires a defaultdict and a key".to_string(),
                span,
            )),
        },
        "collections_put" => match args {
            [dictionary, key, value] => {
                let (mut pairs, factory) = defaultdict_parts(span, dictionary)?;
                match pairs.iter_mut().find(|(candidate, _)| candidate == key) {
                    Some((_, entry)) => *entry = value.clone(),
                    None => pairs.push((key.clone(), value.clone())),
                }
                Ok(Some(defaultdict_value(pairs, factory)))
            }
            _ => Err(Error::Runtime(
                "collections.put requires a defaultdict, a key and a value".to_string(),
                span,
            )),
        },
        // A deque is an opaque object holding two persistent stacks, `front`
        // and `back`. The front stack holds its items in reverse. Each stack is
        // a spine of complete binary trees, so the structural change for a push
        // or pop touches O(log n) nodes instead of rebuilding an n-deep chain of
        // `{value, next}` records. Because source cannot build the object, its
        // trees are trusted after construction; a hand-written record with the
        // same fields is validated all the way down before use instead.
        // Reversing one side into the other when it empties reads that side's
        // pop order once and builds the other side in one pass, so it is linear
        // in the side's length and amortised over the pops that emptied it.
        "collections_deque" => match args {
            [] => Ok(Some(deque_value(
                Value::list(Vec::new()),
                Value::list(Vec::new()),
            ))),
            [Value::List(items)] => {
                if items.len() > MAX_DEQUE_ITEMS {
                    return Err(deque_budget_error(span, items.len()));
                }
                // The stack pops the last item of the list first, so its pop
                // order is the list reversed.
                let pop_order = items.iter().rev().cloned().collect::<Vec<_>>();
                let back = stack_from_values(&pop_order);
                Ok(Some(deque_value(Value::list(Vec::new()), back)))
            }
            _ => Err(Error::Runtime(
                "collections.deque requires zero or one list".to_string(),
                span,
            )),
        },
        "collections_push_left" => match args {
            [deque, value] => {
                let (front, back) = deque_parts(span, deque)?;
                let front = stack_cons(span, &front, value.clone())?;
                let updated = deque_value(front, back);
                deque_parts(span, &updated)?;
                Ok(Some(updated))
            }
            _ => Err(Error::Runtime(
                "collections.push_left requires a deque and a value".to_string(),
                span,
            )),
        },
        "collections_push_right" => match args {
            [deque, value] => {
                let (front, back) = deque_parts(span, deque)?;
                let back = stack_cons(span, &back, value.clone())?;
                let updated = deque_value(front, back);
                deque_parts(span, &updated)?;
                Ok(Some(updated))
            }
            _ => Err(Error::Runtime(
                "collections.push_right requires a deque and a value".to_string(),
                span,
            )),
        },
        "collections_pop_left" => match args {
            [deque] => {
                let (front, back) = deque_parts(span, deque)?;
                let (value, front, back) = match stack_uncons(span, &front)? {
                    Some((value, rest)) => (value, rest, back),
                    None => {
                        let (front, back) = deque_rebalance(span, &back, true)?;
                        match stack_uncons(span, &front)? {
                            Some((value, rest)) => (value, rest, back),
                            None => (
                                Value::Nothing,
                                Value::list(Vec::new()),
                                Value::list(Vec::new()),
                            ),
                        }
                    }
                };
                Ok(Some(deque_pop(value, deque_value(front, back))))
            }
            _ => Err(Error::Runtime(
                "collections.pop_left requires one deque".to_string(),
                span,
            )),
        },
        "collections_pop_right" => match args {
            [deque] => {
                let (front, back) = deque_parts(span, deque)?;
                let (value, front, back) = match stack_uncons(span, &back)? {
                    Some((value, rest)) => (value, front, rest),
                    None => {
                        let (front, back) = deque_rebalance(span, &front, false)?;
                        match stack_uncons(span, &back)? {
                            Some((value, rest)) => (value, front, rest),
                            None => (
                                Value::Nothing,
                                Value::list(Vec::new()),
                                Value::list(Vec::new()),
                            ),
                        }
                    }
                };
                Ok(Some(deque_pop(value, deque_value(front, back))))
            }
            _ => Err(Error::Runtime(
                "collections.pop_right requires one deque".to_string(),
                span,
            )),
        },
        "collections_namedtuple" => match args {
            [Value::List(names), Value::List(values)] => {
                if names.len() != values.len() {
                    return Err(Error::Runtime(
                        format!(
                            "collections.namedtuple needs one value per name: {} names and {} values",
                            names.len(),
                            values.len()
                        ),
                        span,
                    ));
                }
                let mut fields = Fields::new();
                for (index, name) in names.iter().enumerate() {
                    let Value::Text(name) = name else {
                        return Err(Error::Runtime(
                            "collections.namedtuple field names must be text".to_string(),
                            span,
                        ));
                    };
                    if fields.contains_key(name) {
                        return Err(Error::Runtime(
                            format!("collections.namedtuple has duplicate field '{name}'"),
                            span,
                        ));
                    }
                    fields.insert(name.clone(), values[index].clone());
                }
                Ok(Some(Value::record(fields)))
            }
            _ => Err(Error::Runtime(
                "collections.namedtuple requires a list of names and a list of values".to_string(),
                span,
            )),
        },
        "collections_field" => match args {
            [Value::Record(fields), Value::Text(name)] => match fields.get(name) {
                Some(value) => Ok(Some(value.clone())),
                None => Err(Error::Runtime(
                    format!("Named record has no field '{name}'"),
                    span,
                )),
            },
            _ => Err(Error::Runtime(
                "collections.field requires a named record and a text field name".to_string(),
                span,
            )),
        },
        "input" => read_line(args, span).map(|line| Some(Value::Text(line))),
        "random" => {
            let (min, max) = random_range(span, "random", args, 100.0)?;
            random_int(span, min, max).map(Some)
        }
        "random_seed" => {
            let seed = match args {
                [Value::Number(seed)] => *seed,
                _ => {
                    return Err(Error::Runtime(
                        "random_seed requires one number, as in random_seed(42)".to_string(),
                        span,
                    ))
                }
            };
            if !seed.is_finite() {
                return Err(Error::Runtime(
                    format!("random_seed requires a finite number, got {seed}"),
                    span,
                ));
            }
            // A double carries 53 bits of integer and is exact up to `2^53`, so
            // truncating loses no low bit of anything a seed can express, and
            // `as u64` on a negative `i64` reinterprets rather than clamps, so
            // `random_seed(-1)` is a different seed from `random_seed(1)`.
            //
            // Past the ends of `i64` the cast saturates rather than wrapping, so
            // `random_seed(1e300)` is the seed `random_seed(9223372036854775807)`
            // names. That is a stated contract and not an accident: a seed
            // outside the machine integer has no bits left to give, and both
            // bounds are reproducible and distinct from each other.
            seed_random(seed.trunc() as i64 as u64);
            Ok(Some(Value::Nothing))
        }
        // Files module
        "files_read" => {
            let path = match args.first() {
                Some(Value::Text(p)) => p,
                _ => {
                    return Err(Error::Runtime(
                        "files.read requires a text path".to_string(),
                        span,
                    ))
                }
            };
            crate::vfs::read_to_string(path)
                .map(Value::Text)
                .map(Some)
                .map_err(|e| Error::Io(format!("Failed to read '{}': {}", path, e)))
        }
        "files_write" => {
            let (path, content) = match (args.first(), args.get(1)) {
                (Some(Value::Text(p)), Some(Value::Text(c))) => (p, c),
                _ => {
                    return Err(Error::Runtime(
                        "files.write requires two text arguments".to_string(),
                        span,
                    ))
                }
            };
            crate::vfs::write(path, content)
                .map_err(|e| Error::Io(format!("Failed to write '{}': {}", path, e)))?;
            Ok(Some(Value::Nothing))
        }
        "files_append" => {
            let (path, content) = match (args.first(), args.get(1)) {
                (Some(Value::Text(p)), Some(Value::Text(c))) => (p, c),
                _ => {
                    return Err(Error::Runtime(
                        "files.append requires two text arguments".to_string(),
                        span,
                    ))
                }
            };
            crate::vfs::append(path, content)
                .map_err(|e| Error::Io(format!("Failed to append to '{}': {}", path, e)))?;
            Ok(Some(Value::Nothing))
        }
        "files_exists" => {
            let path = match args.first() {
                Some(Value::Text(p)) => p,
                _ => {
                    return Err(Error::Runtime(
                        "files.exists requires a text path".to_string(),
                        span,
                    ))
                }
            };
            Ok(Some(Value::YesNo(crate::vfs::exists(path))))
        }
        "files_lines" => {
            let path = match args.first() {
                Some(Value::Text(p)) => p,
                _ => {
                    return Err(Error::Runtime(
                        "files.lines requires a text path".to_string(),
                        span,
                    ))
                }
            };
            let content = crate::vfs::read_to_string(path)
                .map_err(|e| Error::Io(format!("Failed to read '{}': {}", path, e)))?;
            let lines: Vec<Value> = content
                .lines()
                .map(|l| Value::Text(l.to_string()))
                .collect();
            Ok(Some(Value::list(lines)))
        }
        "files_delete" => {
            let path = match args.first() {
                Some(Value::Text(p)) => p,
                _ => {
                    return Err(Error::Runtime(
                        "files.delete requires a text path".to_string(),
                        span,
                    ))
                }
            };
            crate::vfs::remove_file(path)
                .map_err(|e| Error::Io(format!("Failed to delete '{}': {}", path, e)))?;
            Ok(Some(Value::Nothing))
        }
        "files_copy" => {
            let (from, to) = match (args.first(), args.get(1)) {
                (Some(Value::Text(f)), Some(Value::Text(t))) => (f, t),
                _ => {
                    return Err(Error::Runtime(
                        "files.copy requires two text arguments".to_string(),
                        span,
                    ))
                }
            };
            crate::vfs::copy(from, to)
                .map(|_| Some(Value::Nothing))
                .map_err(|e| Error::Io(format!("Failed to copy '{}' to '{}': {}", from, to, e)))
        }
        "files_rename" => {
            let (from, to) = match (args.first(), args.get(1)) {
                (Some(Value::Text(f)), Some(Value::Text(t))) => (f, t),
                _ => {
                    return Err(Error::Runtime(
                        "files.rename requires two text arguments".to_string(),
                        span,
                    ))
                }
            };
            crate::vfs::rename(from, to)
                .map(|_| Some(Value::Nothing))
                .map_err(|e| Error::Io(format!("Failed to rename '{}' to '{}': {}", from, to, e)))
        }
        // Time module
        "time_now" => {
            let now = since_epoch(now()?, span)?;
            let secs = now.as_secs();
            let nanos = now.subsec_nanos();
            let record = crate::value::Fields::from([
                ("seconds".to_string(), Value::Number(secs as f64)),
                ("nanoseconds".to_string(), Value::Number(nanos as f64)),
            ]);
            Ok(Some(Value::record(record)))
        }
        "time_sleep" => {
            let seconds = match args.first() {
                Some(Value::Number(n)) => *n,
                _ => {
                    return Err(Error::Runtime(
                        "time.sleep requires a number".to_string(),
                        span,
                    ))
                }
            };
            sleep_for(sleep_duration(seconds, span)?, span)?;
            Ok(Some(Value::Nothing))
        }
        "time_format" => {
            let format = match args.get(1) {
                Some(Value::Text(fmt)) => fmt.clone(),
                None => "%Y-%m-%d %H:%M:%S".to_string(),
                Some(_) => {
                    return Err(Error::Runtime(
                        "time.format requires a date or a number and optional text".to_string(),
                        span,
                    ))
                }
            };
            match args.first() {
                Some(Value::Number(timestamp)) => {
                    let secs = seconds_for_format(*timestamp, span)?;
                    let tm = chrono::DateTime::from_timestamp(secs, 0).ok_or_else(|| {
                        Error::Runtime(
                            format!(
                                "time.format cannot represent {timestamp} seconds from \
                                 1970-01-01T00:00:00Z"
                            ),
                            span,
                        )
                    })?;
                    formatted_date(tm, &format, "time.format", span).map(Some)
                }
                Some(Value::Date(date)) => {
                    formatted_date(date.and_utc(), &format, "time.format", span).map(Some)
                }
                _ => Err(Error::Runtime(
                    "time.format requires a date or a number and optional text".to_string(),
                    span,
                )),
            }
        }
        "time_unix" => {
            let date = match args.first() {
                Some(Value::Text(text)) => match parse_iso8601(text, "time.unix", span) {
                    Ok(date) => date,
                    // The old layout was chrono's `%Y-%m-%d %H:%M:%S`, which
                    // read unpadded fields and a leap second. Falling back to
                    // it keeps those callers' answers without widening the ISO
                    // surface `time.parse` documents.
                    Err(error) => parse_legacy_unix_text(text).ok_or(error)?,
                },
                Some(Value::Date(date)) => *date,
                _ => {
                    return Err(Error::Runtime(
                        "time.unix requires a text timestamp or a date".to_string(),
                        span,
                    ))
                }
            };
            let seconds = unix_seconds_for(date, "time.unix", span)?;
            // A Unix timestamp is a real number, so the fraction `time.parse`
            // kept belongs in the answer. Dropping it here made the two
            // builtins disagree: the date value held the nanosecond, but the
            // timestamp named the whole second before it. The fraction is
            // positive because `timestamp` floors, which is what makes a
            // pre-epoch fraction such as -0.5 come out with the right sign.
            let nanos = date.nanosecond();
            let fraction = if nanos >= 1_000_000_000 {
                // A leap second is stored as `:59` plus a nanosecond in the
                // next second. The old layout answered the second that begins
                // it, so keep that answer rather than adding a whole second.
                f64::from(nanos - 1_000_000_000) / 1_000_000_000.0
            } else {
                f64::from(nanos) / 1_000_000_000.0
            };
            let whole = seconds as f64;
            let answer = whole + fraction;
            // At 2024's magnitude an `f64` step is about 0.24 microseconds, so
            // adding `.999999999` can round up to the next whole second. That
            // would make the number name 23:59:59 while the date value still
            // names 23:59:58.999999999. Keep the answer strictly inside the
            // second `timestamp()` floored to.
            let answer = if answer >= whole + 1.0 {
                (whole + 1.0).next_down()
            } else {
                answer
            };
            Ok(Some(Value::Number(answer)))
        }
        "time_parse" => {
            let Some(Value::Text(text)) = args.first() else {
                return Err(Error::Runtime(
                    "time.parse requires a text timestamp and an optional text layout".to_string(),
                    span,
                ));
            };
            if args.len() > 2 {
                return Err(Error::Runtime(
                    "time.parse requires a text timestamp and an optional text layout".to_string(),
                    span,
                ));
            }
            let date = match args.get(1) {
                None => parse_iso8601(text, "time.parse", span)?,
                Some(Value::Text(format)) => parse_with_layout(text, format, "time.parse", span)?,
                Some(_) => {
                    return Err(Error::Runtime(
                        "time.parse requires a text timestamp and an optional text layout"
                            .to_string(),
                        span,
                    ))
                }
            };
            Ok(Some(Value::Date(date)))
        }
        "time_from_unix" => {
            let Some(Value::Number(timestamp)) = args.first() else {
                return Err(Error::Runtime(
                    "time.from_unix requires a number of seconds".to_string(),
                    span,
                ));
            };
            let seconds = seconds_for_instant(*timestamp, "time.from_unix", span)?;
            let instant = chrono::DateTime::from_timestamp(seconds, 0).ok_or_else(|| {
                Error::Runtime(
                    format!(
                        "time.from_unix cannot represent {timestamp} seconds from \
                         1970-01-01T00:00:00Z"
                    ),
                    span,
                )
            })?;
            Ok(Some(Value::Date(instant.naive_utc())))
        }
        "time_date" => Ok(Some(Value::Date(date_from_arguments(
            args,
            "time.date",
            false,
            span,
        )?))),
        "time_datetime" => Ok(Some(Value::Date(date_from_arguments(
            args,
            "time.datetime",
            true,
            span,
        )?))),
        "time_duration" => Ok(Some(Value::Duration(duration_from_arguments(args, span)?))),
        "time_add_years" => {
            let date = expect_date(args.first(), "time.add_years", span)?;
            let amount = whole_argument(args, 1, "time.add_years", "years", span)?;
            Ok(Some(add_calendar(
                span,
                date,
                amount,
                12,
                "time.add_years",
            )?))
        }
        "time_add_months" => {
            let date = expect_date(args.first(), "time.add_months", span)?;
            let amount = whole_argument(args, 1, "time.add_months", "months", span)?;
            Ok(Some(add_calendar(
                span,
                date,
                amount,
                1,
                "time.add_months",
            )?))
        }
        "time_add_days" => {
            let date = expect_date(args.first(), "time.add_days", span)?;
            let amount = whole_argument(args, 1, "time.add_days", "days", span)?;
            Ok(Some(add_fixed(
                span,
                date,
                TimeDelta::try_days(amount),
                "time.add_days",
            )?))
        }
        "time_add_hours" => {
            let date = expect_date(args.first(), "time.add_hours", span)?;
            let amount = whole_argument(args, 1, "time.add_hours", "hours", span)?;
            Ok(Some(add_fixed(
                span,
                date,
                TimeDelta::try_hours(amount),
                "time.add_hours",
            )?))
        }
        "time_add_minutes" => {
            let date = expect_date(args.first(), "time.add_minutes", span)?;
            let amount = whole_argument(args, 1, "time.add_minutes", "minutes", span)?;
            Ok(Some(add_fixed(
                span,
                date,
                TimeDelta::try_minutes(amount),
                "time.add_minutes",
            )?))
        }
        "time_add_seconds" => {
            let date = expect_date(args.first(), "time.add_seconds", span)?;
            let amount = whole_argument(args, 1, "time.add_seconds", "seconds", span)?;
            Ok(Some(add_fixed(
                span,
                date,
                TimeDelta::try_seconds(amount),
                "time.add_seconds",
            )?))
        }
        // Formats module (JSON/CSV)
        "json_parse" => {
            let text = match args.first() {
                Some(Value::Text(s)) => s,
                _ => return Err(Error::Runtime("json.parse requires text".to_string(), span)),
            };
            parse_json(text, span)
                .map(Some)
                .map_err(|e| Error::Runtime(e.to_string(), span))
        }
        "json_stringify" => {
            let value = match args.first() {
                Some(v) => v.clone(),
                _ => {
                    return Err(Error::Runtime(
                        "json.stringify requires a value".to_string(),
                        span,
                    ))
                }
            };
            let text = json_stringify(&value).map_err(|message| Error::Runtime(message, span))?;
            Ok(Some(Value::Text(text)))
        }
        "csv_parse" => {
            let text = match args.first() {
                Some(Value::Text(s)) => s,
                _ => return Err(Error::Runtime("csv.parse requires text".to_string(), span)),
            };
            parse_csv(text, span).map(Some)
        }
        // Network module
        "network_get" => {
            let url = match args.first() {
                Some(Value::Text(u)) => u,
                _ => {
                    return Err(Error::Runtime(
                        "network.get requires a URL".to_string(),
                        span,
                    ))
                }
            };
            network_get(url, span).map(Some)
        }
        "network_post" => {
            let (url, data) = match (args.first(), args.get(1)) {
                (Some(Value::Text(u)), Some(Value::Text(d))) => (u, d),
                _ => {
                    return Err(Error::Runtime(
                        "network.post requires URL and data".to_string(),
                        span,
                    ))
                }
            };
            network_post(url, data, span).map(Some)
        }
        // Testing module
        "expect" | "assert" => {
            let (actual, expected) = match (args.first(), args.get(1)) {
                (Some(a), Some(e)) => (a.clone(), e.clone()),
                _ => {
                    return Err(Error::Runtime(
                        "expect requires two arguments".to_string(),
                        span,
                    ))
                }
            };
            // The same library call the `expect` statement and the bytecode VM
            // use, so a failing assertion reads identically however it was
            // reached.
            crate::testing::assertions::assert_values_equal(&expected, &actual)
                .map_err(|failure| Error::Runtime(failure.to_string(), span))?;
            Ok(Some(Value::Nothing))
        }
        // Console module
        "console_log" => {
            if let Some(arg) = args.first() {
                // The same boundary `say` uses, for the same reason: a page that
                // can collect `say` but not `console.log` collected half a
                // program's output.
                crate::wasm::emit(&format!("{}\n", arg));
            }
            Ok(Some(Value::Nothing))
        }
        "console_error" => {
            if let Some(arg) = args.first() {
                // Deliberately *not* the capture. stderr is where a program's
                // diagnostics go and stdout is where its output goes; folding
                // the two together would change what the native binary writes on
                // each stream, which is the thing the playground promises not to
                // do. The page has its own surface for it.
                eprintln!("{}", arg);
            }
            Ok(Some(Value::Nothing))
        }
        "console_clear" => {
            crate::wasm::emit("\x1B[2J\x1B[1H");
            Ok(Some(Value::Nothing))
        }
        // Random module
        "random_number" => {
            let (min, max) = random_range(span, "random_number", args, 1.0)?;
            random_float(span, min, max).map(Some)
        }
        "random_choice" => {
            if let Some(Value::List(items)) = args.first() {
                if items.is_empty() {
                    // An empty list has no member to choose, so there is no
                    // answer to give. `nothing` used to be handed back, which is
                    // a value the program could not tell apart from a choice
                    // that had not happened yet.
                    return Err(Error::Runtime(
                        "random_choice requires a list with at least one element".to_string(),
                        span,
                    ));
                }
                let idx = (next_bits() % items.len() as u64) as usize;
                Ok(Some(items[idx].clone()))
            } else {
                Err(Error::Runtime(
                    "random_choice requires a list".to_string(),
                    span,
                ))
            }
        }
        "random_shuffle" => {
            if let Some(Value::List(items)) = args.first().cloned() {
                let mut shuffled = (*items).clone();
                // Fisher-Yates, drawing a fresh index at each step. It used to
                // compute one `seed` and use `seed % (i + 1)` for every swap,
                // which is not a shuffle at all: the permutation was a fixed
                // function of one clock reading, the same for every input of the
                // same length.
                for i in (1..shuffled.len()).rev() {
                    let j = (next_bits() % (i as u64 + 1)) as usize;
                    shuffled.swap(i, j);
                }
                Ok(Some(Value::list(shuffled)))
            } else {
                Err(Error::Runtime(
                    "random_shuffle requires a list".to_string(),
                    span,
                ))
            }
        }
        // Type conversion
        "type_of" => {
            let type_name = args.first().map(|v| v.type_name()).unwrap_or("nothing");
            Ok(Some(Value::Text(type_name.to_string())))
        }
        // The conversion and list builtins `stdlib::builtins` registers and
        // this function did not answer. A program could name every one of them
        // and each call failed with `Unknown function`, which is the message for
        // a name nothing implements — not for a name the language documents.
        "to_number" => {
            let text = match args.first() {
                Some(Value::Text(text)) => text,
                _ => {
                    return Err(Error::Runtime(
                        "to_number requires a text argument".to_string(),
                        span,
                    ))
                }
            };
            // The same parse the lexer does, so a number written in a program
            // and the same number written as text are one value bit for bit.
            let number: f64 = text
                .trim()
                .parse()
                .map_err(|_| Error::Runtime(format!("Cannot read '{text}' as a number"), span))?;
            crate::value::finite_number(number, span)
                .map(Value::Number)
                .map(Some)
        }
        "to_text" => {
            let text = match args.first() {
                Some(value) => value.to_string(),
                None => {
                    return Err(Error::Runtime(
                        "to_text requires an argument".to_string(),
                        span,
                    ))
                }
            };
            Ok(Some(Value::Text(text)))
        }
        "push" => {
            let (items, value) = match (args.first(), args.get(1)) {
                (Some(Value::List(items)), Some(value)) => (items, value),
                _ => {
                    return Err(Error::Runtime(
                        "push requires a list and a value".to_string(),
                        span,
                    ))
                }
            };
            let mut pushed = items.clone();
            crate::value::Shared::make_mut(&mut pushed).push(value.clone());
            Ok(Some(Value::List(pushed)))
        }
        // `bytes.from_text` and `bytes.write` are the whole of a binary file
        // API, and there was none: `files.write` writes the UTF-8 of a text, so
        // a program had no way to write a byte it could not spell. That is not
        // a gap a self-hosted compiler can be written around — its output is
        // bytes — so it is the minimum binary output a language with a
        // bytecode format needs.
        "bytes_from_text" => {
            let text = match args.first() {
                Some(Value::Text(text)) => text,
                _ => {
                    return Err(Error::Runtime(
                        "bytes.from_text requires a text argument".to_string(),
                        span,
                    ))
                }
            };
            Ok(Some(Value::list(
                text.as_bytes()
                    .iter()
                    .map(|byte| Value::Number(*byte as f64))
                    .collect(),
            )))
        }
        "bytes_write" => {
            let (path, bytes) = match (args.first(), args.get(1)) {
                (Some(Value::Text(path)), Some(Value::List(bytes))) => (path, bytes),
                _ => {
                    return Err(Error::Runtime(
                        "bytes.write requires a path and a list of byte values".to_string(),
                        span,
                    ))
                }
            };
            let mut out = Vec::with_capacity(bytes.len());
            for byte in bytes.iter() {
                let Value::Number(value) = byte else {
                    return Err(Error::Runtime(
                        format!(
                            "bytes.write was given a {} where a byte belongs",
                            byte.type_name()
                        ),
                        span,
                    ));
                };
                // A byte is 0..=255, and the check is on the value rather than
                // on a cast, so `bytes.write("f", [256])` is a refusal with a
                // message instead of a file holding a wrapped zero.
                if !value.is_finite() || value.fract() != 0.0 || *value < 0.0 || *value > 255.0 {
                    return Err(Error::Runtime(
                        format!("bytes.write was given {value}, which is not a byte (0 to 255)"),
                        span,
                    ));
                }
                out.push(*value as u8);
            }
            // Through the filesystem shim, like `files.write`: `std::fs` here
            // would compile for `wasm32-unknown-unknown`, pass every native test
            // and fail at runtime there, leaving the two write paths pointing at
            // two different filesystems in the one build that has both.
            crate::vfs::write_bytes(path, &out)
                .map_err(|e| Error::Io(format!("Failed to write '{}': {}", path, e)))?;
            Ok(Some(Value::Nothing))
        }
        "bytes_text" => {
            // The inverse of `bytes.from_text`, so a program that reads a byte
            // list does not have to give up and start again with text: the
            // self-hosted compiler lexes bytes and still needs to say what a
            // name is.
            let bytes = match args.first() {
                Some(Value::List(bytes)) => bytes,
                _ => {
                    return Err(Error::Runtime(
                        "bytes.text requires a list of byte values".to_string(),
                        span,
                    ))
                }
            };
            let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
            for byte in bytes.iter() {
                let Value::Number(value) = byte else {
                    return Err(Error::Runtime(
                        format!(
                            "bytes.text was given a {} where a byte belongs",
                            byte.type_name()
                        ),
                        span,
                    ));
                };
                if !value.is_finite() || value.fract() != 0.0 || *value < 0.0 || *value > 255.0 {
                    return Err(Error::Runtime(
                        format!("bytes.text was given {value}, which is not a byte (0 to 255)"),
                        span,
                    ));
                }
                out.push(*value as u8);
            }
            // Not every byte list is text, and the failure names the byte
            // rather than reporting a replacement character three files later.
            match String::from_utf8(out) {
                Ok(text) => Ok(Some(Value::Text(text))),
                Err(e) => Err(Error::Runtime(
                    format!("bytes.text was given bytes that are not text: {e}"),
                    span,
                )),
            }
        }
        "sys_argv" => Ok(Some(Value::list(
            program_args()
                .iter()
                .map(|argument| Value::Text(argument.clone()))
                .collect(),
        ))),
        _ => Ok(None),
    }
}

/// The arguments after the program path, for `sys.argv()`.
///
/// The CLI sets them when it runs a file and nothing else does, so a program run
/// from the REPL or from a test sees an empty list rather than the arguments of
/// whatever process happens to be running.
fn program_args() -> Vec<String> {
    PROGRAM_ARGS
        .lock()
        .map(|args| args.clone())
        .unwrap_or_default()
}

/// Where `sys.argv()` reads from. See [`set_program_args`].
static PROGRAM_ARGS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Records the arguments that follow the program path, for `sys.argv()`.
///
/// A program has no other way to be told what it was asked to do: `rb run` takes
/// a path and nothing else, so a compiler written in Redblue could not be given
/// a file to compile.
pub fn set_program_args(args: Vec<String>) {
    if let Ok(mut slot) = PROGRAM_ARGS.lock() {
        *slot = args;
    }
}

/// The arguments [`set_program_args`] recorded, for a caller that has to put
/// back what it found.
///
/// Setting them is one-way: a caller that publishes arguments of its own over
/// someone else's — the bootstrap ladder's stage 2, which runs a compiler with
/// an input and an output path — would otherwise leave its paths in place for
/// whatever reads `sys.argv()` next in the same process. Reading them here is
/// what lets it save, set, and restore.
pub fn take_program_args() -> Vec<String> {
    std::mem::take(
        &mut *PROGRAM_ARGS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()),
    )
}

/// The whole-request timeout for `network.get` and `network.post`: connect,
/// send, headers and body read together. It is here because
/// `reqwest::blocking::Client::new()` has no timeout of its own, and a client
/// without one waits for the operating system's own connect timeout — about
/// two and a half minutes on Linux, and forever on a connection that is
/// accepted and never answered.
pub const NETWORK_TIMEOUT_SECS: u64 = 10;

/// The connect timeout for the same two functions, so a host that drops
/// packets is given up on before the whole-request timeout is reached.
pub const NETWORK_CONNECT_TIMEOUT_SECS: u64 = 5;

/// Builds the client every `network` call uses. A client that cannot be built
/// is a `Runtime` error rather than a panic: a Redblue program must not be able
/// to abort the process.
#[cfg(not(target_arch = "wasm32"))]
fn network_client(span: Span) -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(NETWORK_TIMEOUT_SECS))
        .connect_timeout(std::time::Duration::from_secs(NETWORK_CONNECT_TIMEOUT_SECS))
        .build()
        .map_err(|e| Error::Runtime(format!("Cannot create the HTTP client: {}", e), span))
}

/// The WebAssembly playground has no threads, so `reqwest`'s blocking client —
/// which is compiled out of that target entirely — cannot be built there. The
/// two `network` builtins answer with a clean `Runtime` error instead, naming
/// what is missing and what to use instead. Nothing else about them changes:
/// same names, same arity, same argument-type refusals, same `Error::Runtime`
/// shape. Only the transport differs, and the transport is the whole of what a
/// playground cannot have.
#[cfg(target_arch = "wasm32")]
fn network_refused(call: &str, span: Span) -> Error {
    Error::Runtime(
        format!(
            "network.{call} is not available in the WebAssembly playground: it has no \
             threads to block on. Use the native rb binary, or run the program on a host \
             that can reach the network."
        ),
        span,
    )
}

#[cfg(not(target_arch = "wasm32"))]
fn network_get(url: &str, span: Span) -> Result<Value> {
    let client = network_client(span)?;
    let response = client
        .get(url)
        .send()
        .map_err(|e| Error::Runtime(format!("HTTP request failed: {}", e), span))?;
    let body = response
        .text()
        .map_err(|e| Error::Runtime(format!("Failed to read response: {}", e), span))?;
    Ok(Value::Text(body))
}

#[cfg(target_arch = "wasm32")]
fn network_get(_url: &str, span: Span) -> Result<Value> {
    Err(network_refused("get", span))
}

#[cfg(not(target_arch = "wasm32"))]
fn network_post(url: &str, data: &str, span: Span) -> Result<Value> {
    let client = network_client(span)?;
    let response = client
        .post(url)
        .body(data.to_string())
        .send()
        .map_err(|e| Error::Runtime(format!("HTTP request failed: {}", e), span))?;
    let body = response
        .text()
        .map_err(|e| Error::Runtime(format!("Failed to read response: {}", e), span))?;
    Ok(Value::Text(body))
}

#[cfg(target_arch = "wasm32")]
fn network_post(_url: &str, _data: &str, span: Span) -> Result<Value> {
    Err(network_refused("post", span))
}

fn parse_json(json: &str, span: Span) -> Result<Value> {
    let json = json.trim();
    if json.starts_with('{') {
        parse_json_object(json, span)
    } else if json.starts_with('[') {
        parse_json_array(json, span)
    } else if json.starts_with('"') {
        Ok(Value::Text(parse_json_string(json, span)?))
    } else if json == "null" {
        Ok(Value::Nothing)
    } else if json == "true" {
        Ok(Value::YesNo(true))
    } else if json == "false" {
        Ok(Value::YesNo(false))
    } else {
        match json.parse::<f64>() {
            Ok(n) => Value::number(n, span),
            Err(_) => Err(Error::Runtime(format!("Invalid JSON: {}", json), span)),
        }
    }
}

/// Reads four hex digits of a `\uXXXX` escape starting at `at`, which is the
/// index of the escape's first digit.
fn json_hex4(chars: &[char], at: usize, span: Span) -> Result<u32> {
    let mut value = 0u32;
    for offset in 0..4 {
        let digit = chars
            .get(at + offset)
            .and_then(|c| c.to_digit(16))
            .ok_or_else(|| {
                Error::Runtime(
                    "Invalid JSON escape: '\\u' needs four hex digits".to_string(),
                    span,
                )
            })?;
        value = value * 16 + digit;
    }
    Ok(value)
}

/// Decodes the `\uXXXX` escape whose backslash is at `at`, returning the
/// character and the index just past it. A character outside the Basic
/// Multilingual Plane is written as a surrogate pair, and a half of one is an
/// error rather than a replacement character.
fn decode_json_unicode_escape(chars: &[char], at: usize, span: Span) -> Result<(char, usize)> {
    let unpaired = || {
        Error::Runtime(
            "Invalid JSON escape: unpaired surrogate in '\\u' escape".to_string(),
            span,
        )
    };
    let first = json_hex4(chars, at + 2, span)?;
    match first {
        0xD800..=0xDBFF => {
            if chars.get(at + 6) != Some(&'\\') || chars.get(at + 7) != Some(&'u') {
                return Err(unpaired());
            }
            let second = json_hex4(chars, at + 8, span)?;
            if !(0xDC00..=0xDFFF).contains(&second) {
                return Err(unpaired());
            }
            let combined = 0x10000 + ((first - 0xD800) << 10) + (second - 0xDC00);
            Ok((char::from_u32(combined).ok_or_else(unpaired)?, at + 12))
        }
        0xDC00..=0xDFFF => Err(unpaired()),
        _ => Ok((
            char::from_u32(first)
                .ok_or_else(|| Error::Runtime("Invalid JSON escape".to_string(), span))?,
            at + 6,
        )),
    }
}

/// Parses CSV the way the format spells itself: a field whose first character
/// is `"` is quoted, and inside a quoted field a comma, a newline and `""` are
/// data rather than structure. A bare field keeps its old meaning — trimmed of
/// surrounding whitespace — because that is what the language shipped.
///
/// Rows may be ragged: each row is returned with the cells it actually has, so
/// a short row is a short row rather than an error.
fn parse_csv(text: &str, span: Span) -> Result<Value> {
    let chars: Vec<char> = text.chars().collect();
    let mut rows: Vec<Value> = Vec::new();
    let mut row: Vec<Value> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut was_quoted = false;
    let mut quote_closed = false;
    let mut at_field_start = true;
    let mut index = 0;

    while index < chars.len() {
        let c = chars[index];
        if in_quotes {
            if c == '"' && chars.get(index + 1) == Some(&'"') {
                field.push('"');
                index += 2;
                continue;
            }
            if c != '"' {
                field.push(c);
                index += 1;
                continue;
            }
            in_quotes = false;
            quote_closed = true;
            index += 1;
            continue;
        }
        if quote_closed {
            // Whatever sits between the closing quote and the separator is not
            // part of the field: the quote already said where the field ends.
            if c == ',' || c == '\n' || c == '\r' {
                quote_closed = false;
            } else {
                index += 1;
                continue;
            }
        }
        match c {
            '"' if at_field_start => {
                in_quotes = true;
                was_quoted = true;
                at_field_start = false;
            }
            ',' | '\n' | '\r' => {
                let cell = if was_quoted {
                    field.clone()
                } else {
                    field.trim().to_string()
                };
                row.push(Value::Text(cell));
                field.clear();
                was_quoted = false;
                at_field_start = true;
                if c != ',' {
                    // A lone CR and the CR of a CRLF pair both end the row, so a
                    // file written on either platform reads the same.
                    if c == '\r' && chars.get(index + 1) == Some(&'\n') {
                        index += 1;
                    }
                    rows.push(Value::list(std::mem::take(&mut row)));
                }
            }
            _ => {
                field.push(c);
                at_field_start = false;
            }
        }
        index += 1;
    }

    if in_quotes {
        return Err(Error::Runtime(
            "Invalid CSV: unterminated quoted field".to_string(),
            span,
        ));
    }
    if at_field_start && row.is_empty() {
        // The text ended on a row separator, so there is no trailing empty row
        // — and empty text has no rows at all.
        return Ok(Value::list(rows));
    }
    let cell = if was_quoted {
        field.clone()
    } else {
        field.trim().to_string()
    };
    row.push(Value::Text(cell));
    rows.push(Value::list(row));
    Ok(Value::list(rows))
}

fn parse_json_object(json: &str, span: Span) -> Result<Value> {
    let json = json.trim();
    if !json.starts_with('{') || !json.ends_with('}') {
        return Err(Error::Runtime("Invalid JSON object".to_string(), span));
    }
    let mut map = crate::value::Fields::new();
    let content = &json[1..json.len() - 1];
    if content.trim().is_empty() {
        return Ok(Value::record(map));
    }
    for pair in split_json_pairs(content) {
        let parts: Vec<&str> = pair.splitn(2, ':').collect();
        if parts.len() != 2 {
            // A pair with no colon is malformed input, not a pair to skip: it
            // used to vanish and leave a record that was missing its field.
            if !pair.trim().is_empty() {
                return Err(Error::Runtime(
                    "Invalid JSON object: expected \'key: value\'".to_string(),
                    span,
                ));
            }
            continue;
        }
        let key = parse_json_string(parts[0].trim(), span)?;
        let value = parse_json(parts[1].trim(), span)?;
        map.insert(key, value);
    }
    Ok(Value::record(map))
}

fn parse_json_array(json: &str, span: Span) -> Result<Value> {
    let json = json.trim();
    if !json.starts_with('[') || !json.ends_with(']') {
        return Err(Error::Runtime("Invalid JSON array".to_string(), span));
    }
    let content = &json[1..json.len() - 1];
    if content.trim().is_empty() {
        return Ok(Value::list(Vec::new()));
    }
    let mut items = Vec::new();
    for item in split_json_elements(content) {
        items.push(parse_json(item, span)?);
    }
    Ok(Value::list(items))
}

fn parse_json_string(json: &str, span: Span) -> Result<String> {
    let json = json.trim();
    if !json.starts_with('"') || !json.ends_with('"') || json.len() < 2 {
        return Err(Error::Runtime("Invalid JSON string".to_string(), span));
    }
    let chars: Vec<char> = json[1..json.len() - 1].chars().collect();
    let mut result = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '\\' {
            result.push(chars[i]);
            i += 1;
            continue;
        }
        let escape = *chars.get(i + 1).ok_or_else(|| {
            Error::Runtime(
                "Invalid JSON escape: string ends with '\\'".to_string(),
                span,
            )
        })?;
        match escape {
            'n' => result.push('\n'),
            't' => result.push('\t'),
            'r' => result.push('\r'),
            'b' => result.push('\u{8}'),
            'f' => result.push('\u{c}'),
            '"' => result.push('"'),
            '\\' => result.push('\\'),
            'u' => {
                let (character, next) = decode_json_unicode_escape(&chars, i, span)?;
                result.push(character);
                i = next;
                continue;
            }
            other => {
                return Err(Error::Runtime(
                    format!("Invalid JSON escape: '\\{}'", other),
                    span,
                ))
            }
        }
        i += 2;
    }
    Ok(result)
}

/// Escapes `text` for a JSON string body: the quote, the backslash and every
/// control character that JSON requires be written as an escape.
///
/// `pub(crate)` because the REPL's own JSON printer writes the same document
/// and must escape it the same way: two escapers would be two answers to what
/// a text with a quote in it looks like in JSON.
pub(crate) fn json_escape_text(text: &str) -> String {
    let mut result = String::from("\"");
    for c in text.chars() {
        match c {
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            '\u{8}' => result.push_str("\\b"),
            '\u{c}' => result.push_str("\\f"),
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            other => {
                if (other as u32) < 0x20 {
                    result.push_str(&format!("\\u{:04x}", other as u32));
                } else {
                    result.push(other);
                }
            }
        }
    }
    result.push('"');
    result
}

fn split_json_pairs(content: &str) -> Vec<&str> {
    let mut pairs = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    let mut in_string = false;
    let mut prev_char: Option<char> = None;
    for (i, c) in content.char_indices() {
        if c == '"' && prev_char != Some('\\') {
            in_string = !in_string;
        }
        if !in_string {
            if c == '{' || c == '[' {
                depth += 1;
            } else if c == '}' || c == ']' {
                depth -= 1;
            } else if c == ',' && depth == 0 {
                pairs.push(&content[start..i]);
                start = i + c.len_utf8();
            }
        }
        prev_char = Some(c);
    }
    if start < content.len() {
        pairs.push(&content[start..]);
    }
    pairs
}

fn split_json_elements(content: &str) -> Vec<&str> {
    split_json_pairs(content)
}

/// Writes `value` as JSON, on one line, or refuses an opaque deque.
///
/// `pub(crate)` because the REPL's `.json` command prints the same document:
/// one writer means the REPL and `json.stringify` cannot disagree about what
/// a value looks like in JSON. A deque has no JSON form, and writing `null`
/// for one would say it is `nothing`, so it is refused instead.
pub(crate) fn json_stringify(value: &Value) -> std::result::Result<String, String> {
    match value {
        Value::Nothing => Ok("null".to_string()),
        Value::YesNo(b) => {
            if *b {
                Ok("true".to_string())
            } else {
                Ok("false".to_string())
            }
        }
        Value::Number(n) => {
            // JSON has no literal for a number that is not finite, so it is
            // written as `null`, which is what every JSON writer emits for one.
            // No Redblue program can reach this branch: see SPEC.md.
            if !n.is_finite() {
                Ok("null".to_string())
            } else if n.fract() == 0.0 && n.abs() < 1e15 {
                Ok(format!("{}", *n as i64))
            } else {
                Ok(format!("{}", n))
            }
        }
        Value::Text(s) => Ok(json_escape_text(s)),
        Value::List(items) => {
            let elements = items
                .iter()
                .map(json_stringify)
                .collect::<std::result::Result<Vec<_>, _>>()?;
            Ok(format!("[{}]", elements.join(", ")))
        }
        Value::Record(fields) => {
            let mut pairs = Vec::with_capacity(fields.len());
            for (key, field) in fields.iter() {
                pairs.push(format!(
                    "{}: {}",
                    json_escape_text(key),
                    json_stringify(field)?
                ));
            }
            Ok(format!("{{{}}}", pairs.join(", ")))
        }
        Value::Function(_) => Ok("null".to_string()),
        Value::Builtin(_) => Ok("null".to_string()),
        Value::Object(name, _) if name == "deque" => Err(
            "JSON has no literal for a deque, and null would say 'nothing' about a \
             value that is not nothing"
                .to_string(),
        ),
        Value::Object(_, _) => Ok("null".to_string()),
        Value::Date(date) => Ok(json_escape_text(&crate::value::date_text(*date))),
        Value::Duration(duration) => Ok(format!("{}", duration.as_seconds_f64())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// JSON has no literal for a number that is not finite. A host program
    /// embedding Redblue can still build one through the public `Value`, so the
    /// writer has to answer for it, and `null` is the answer every JSON writer
    /// gives. SPEC.md states this.
    #[test]
    fn edge_json_writes_a_number_that_is_not_finite_as_null() {
        for (value, what) in [
            (f64::NAN, "NaN"),
            (f64::INFINITY, "infinity"),
            (f64::NEG_INFINITY, "negative infinity"),
        ] {
            assert_eq!(
                json_stringify(&Value::Number(value)).unwrap(),
                "null",
                "{} must be written as null, not as a JSON number",
                what
            );
        }
        // The widest finite numbers are numbers, and must not be nulled.
        for finite in [f64::MAX, f64::MIN, f64::MIN_POSITIVE, 0.0, -0.0, 1.5] {
            let written = json_stringify(&Value::Number(finite)).unwrap();
            assert_ne!(written, "null", "{} should still be written", finite);
        }
        assert_eq!(json_stringify(&Value::Number(42.0)).unwrap(), "42");
        assert_eq!(json_stringify(&Value::Number(1.5)).unwrap(), "1.5");
    }

    /// A cache key cannot use the language's `is` for a function: that compares
    /// name, parameters and captured bindings, so two declarations can produce
    /// same-shaped functions with different bodies. A function in a key is
    /// matched by identity, including when it is nested in a list.
    #[test]
    fn edge_lru_cache_keys_function_arguments_by_identity() {
        use crate::value::{FunctionBody, FunctionValue};
        use std::sync::Arc;

        let function = |name: &str| {
            Value::Function(FunctionValue::new(
                name.to_string(),
                vec!["x".to_string()],
                FunctionBody::Statements(Arc::new(Vec::new())),
                Arc::new(Vec::new()),
            ))
        };
        let first = function("f");
        let second = function("f");
        assert_eq!(
            first, second,
            "the language's `is` treats two same-shaped functions as equal"
        );

        let mut cache = LruCache::new(4);
        cache.insert(&[first.clone(), Value::Number(1.0)], Value::Number(2.0));
        assert_eq!(
            cache.get(&[second.clone(), Value::Number(1.0)]),
            None,
            "a distinct function value must not answer from another's entry"
        );

        cache.insert(
            &[Value::list(vec![first]), Value::Number(1.0)],
            Value::Number(3.0),
        );
        assert_eq!(
            cache.get(&[Value::list(vec![second]), Value::Number(1.0)]),
            None,
            "a function nested in a list key is still matched by identity"
        );
    }

    /// A clock set before 1970 is the one host state that makes every clock
    /// reading impossible. It must be a `Runtime` error naming the clock, not a
    /// panic — a panic here unwinds the interpreter thread, and the program
    /// that merely asked for the time would lose the process instead of
    /// receiving something it could `catch`.
    ///
    /// The instant is passed in, so the test does not move the machine's clock.
    #[test]
    fn edge_a_clock_before_the_epoch_is_a_runtime_error() {
        let one_second_before = UNIX_EPOCH - Duration::from_secs(1);
        match since_epoch(one_second_before, Span::new(3, 9)) {
            Err(Error::Runtime(message, span)) => {
                assert!(
                    message.contains("clock"),
                    "the message must name the clock, got `{}`",
                    message
                );
                assert_eq!(
                    span,
                    Span::new(3, 9),
                    "the failure must be reported against the call site"
                );
            }
            Ok(since) => panic!("a pre-epoch clock must fail, got {:?}", since),
            other => panic!("expected a Runtime error, got {:?}", other),
        }
        // One nanosecond before the epoch is the closest pre-epoch instant there
        // is, so the boundary is tested at its own edge rather than one second
        // away from it.
        let one_nanosecond_before = UNIX_EPOCH - Duration::from_nanos(1);
        assert!(
            since_epoch(one_nanosecond_before, Span::new(1, 1)).is_err(),
            "one nanosecond before the epoch is still before it"
        );
    }

    /// The epoch itself is zero seconds after itself, not an error. The two
    /// tests above and this one are what make the conversion total: the boundary
    /// is exactly where it is stated to be.
    #[test]
    fn the_epoch_boundary_is_accepted_by_the_conversion() {
        assert_eq!(
            since_epoch(UNIX_EPOCH, Span::new(1, 1)).expect("the epoch is an instant"),
            Duration::ZERO,
            "the epoch is zero seconds after itself"
        );
        assert_eq!(
            since_epoch(UNIX_EPOCH + Duration::from_secs(1), Span::new(1, 1))
                .expect("one second after the epoch is an instant"),
            Duration::from_secs(1),
            "one second after the epoch is one second after it"
        );
    }

    /// A timestamp that names no instant is refused rather than saturated. The
    /// saturating `as u64` cast this replaced turned `time.format(1e300)` into
    /// a panic and `time.format(-1)` into `1970-01-01`, which is a wrong answer
    /// dressed as a right one.
    #[test]
    fn edge_a_timestamp_naming_no_instant_is_refused() {
        let span = Span::new(2, 5);
        for (timestamp, what) in [
            (-1e300, "far before any date chrono can name"),
            (1e300, "far after any date"),
            (253_402_300_801.0, "one second past the year 10000"),
            (f64::NAN, "NaN, which is not a number of seconds"),
            (f64::INFINITY, "infinity"),
            (f64::NEG_INFINITY, "negative infinity"),
        ] {
            match seconds_for_format(timestamp, span) {
                Err(Error::Runtime(message, _)) => assert!(
                    message.contains("time.format"),
                    "{} must be refused as a time.format error, got `{}`",
                    what,
                    message
                ),
                Ok(secs) => panic!("{} must be refused, got {} seconds", what, secs),
                other => panic!("{} should be a Runtime error, got {:?}", what, other),
            }
        }
    }

    /// A timestamp before the epoch names a real date, so it is formatted and
    /// not refused. The zero boundary is where the refusal starts, and a
    /// conversion that answered `1970-01-01` for `1969-12-31T23:59:59Z` would
    /// be wrong by a day and never say so.
    #[test]
    fn a_pre_epoch_timestamp_is_a_date_not_an_error() {
        for (timestamp, expected, what) in [
            (-1.0, "1969-12-31", "one second before 1970"),
            (-86_400.0, "1969-12-31", "the whole of 1969-12-31"),
            (-2_208_988_800.0, "1900-01-01", "the start of 1900"),
        ] {
            let secs = seconds_for_format(timestamp, Span::new(1, 1))
                .unwrap_or_else(|e| panic!("{} is a real date, got {:?}", what, e));
            assert_eq!(
                chrono::DateTime::from_timestamp(secs, 0)
                    .unwrap_or_else(|| panic!("{} should be formattable", what))
                    .format("%Y-%m-%d")
                    .to_string(),
                expected,
                "{} should format as the date it names",
                what
            );
        }
    }

    /// A number of seconds `Duration::from_secs_f64` panics on must be refused
    /// before it reaches `thread::sleep`. The red test was a panic in `core` at
    /// `time.rs:962` — "cannot convert float seconds to Duration: value is
    /// either too big or NaN" — which unwound the interpreter thread and
    /// replaced the program's failure with "The interpreter thread stopped
    /// unexpectedly", uncatchable by a Redblue `try`.
    #[test]
    fn edge_a_sleep_the_clock_cannot_wait_is_a_runtime_error() {
        let span = Span::new(4, 12);
        for (seconds, what) in [
            (-1.0, "a negative number of seconds"),
            (-0.5, "a negative fraction of a second"),
            (f64::NEG_INFINITY, "negative infinity"),
            (f64::NAN, "NaN, which is not a number of seconds"),
            (f64::INFINITY, "infinity"),
            (1e20, "a number of seconds no clock can wait"),
            (1e300, "far longer than any clock can wait"),
            (f64::MAX, "the widest finite number"),
        ] {
            match sleep_duration(seconds, span) {
                Err(Error::Runtime(message, _)) => assert!(
                    message.contains("time.sleep"),
                    "{} must be refused as a time.sleep error, got `{}`",
                    what,
                    message
                ),
                Ok(duration) => panic!("{} must be refused, got {:?}", what, duration),
                other => panic!("should be a Runtime error, got {:?}", other),
            }
        }
    }

    /// A sleep `time.sleep` can actually perform is not refused. A zero-second
    /// sleep is the boundary the refusals above sit against, and a fractional
    /// one is a wait that has always worked, so neither may become an error
    /// here — a conversion that answered "you may not sleep" for `0` would
    /// refuse a program that used to run.
    #[test]
    fn a_sleep_within_the_waitable_range_is_accepted() {
        for (seconds, expected, what) in [
            (0.0, Duration::ZERO, "no wait at all"),
            (-0.0, Duration::ZERO, "negative zero is zero"),
            (0.25, Duration::from_millis(250), "a quarter second"),
            (1.5, Duration::from_millis(1500), "a second and a half"),
            (10.0, Duration::from_secs(10), "ten whole seconds"),
        ] {
            assert_eq!(
                sleep_duration(seconds, Span::new(1, 1))
                    .unwrap_or_else(|e| panic!("{} must be waitable, got {:?}", what, e)),
                expected,
                "{} must reach thread::sleep as the duration it names",
                what
            );
        }
    }

    /// `chrono` will parse a year as low as -262143, and the seconds that come
    /// back are ones `time.format` refuses — the two builtins disagreed about
    /// whether the instant existed. A date outside the window both accept is a
    /// `Runtime` error naming `time.unix`, not a number nothing else will take.
    #[test]
    fn edge_a_date_outside_the_window_both_builtins_accept_is_refused() {
        let span = Span::new(1, 1);
        for (text, what) in [
            ("-99999-01-01 00:00:00", "a year past what a date can name"),
            ("-262143-01-01 00:00:00", "the earliest year chrono parses"),
            (
                "-9999-01-01 00:00:00",
                "before the window time.format accepts",
            ),
        ] {
            let date = parse_iso8601(text, "time.unix", span)
                .unwrap_or_else(|error| panic!("{} should parse first, got {:?}", what, error));
            match unix_seconds_for(date, "time.unix", span) {
                Err(Error::Runtime(message, _)) => assert!(
                    message.contains("time.unix"),
                    "{} must be refused as a time.unix error, got `{}`",
                    what,
                    message
                ),
                Ok(secs) => panic!("{} must be refused, got {} seconds", what, secs),
                other => panic!("should be a Runtime error, got {:?}", other),
            }
        }
    }

    /// A date inside the window is a timestamp, and the same instant read back
    /// through `time.format` must name it again. This is what makes the two
    /// builtins agree rather than merely both return a number.
    #[test]
    fn a_date_inside_the_window_round_trips_through_time_format() {
        for (text, expected) in [
            ("1970-01-01 00:00:00", 0_i64),
            ("1969-12-31 23:59:59", -1),
            ("2024-01-15 12:30:00", 1_705_321_800),
            ("9999-12-31 23:59:59", 253_402_300_799),
            ("+10000-01-01 00:00:00", 253_402_300_800),
        ] {
            let date = parse_iso8601(text, "time.unix", Span::new(1, 1))
                .unwrap_or_else(|e| panic!("{} should parse, got {:?}", text, e));
            let seconds = unix_seconds_for(date, "time.unix", Span::new(1, 1))
                .unwrap_or_else(|e| panic!("{} is a real date, got {:?}", text, e));
            assert_eq!(seconds, expected, "{} should count from the epoch", text);
            // The point of the shared window: what `time.unix` produces,
            // `time.format` must accept.
            assert!(
                seconds_for_format(seconds as f64, Span::new(1, 1)).is_ok(),
                "{} must produce a timestamp time.format can read back",
                text
            );
        }
    }
}

/// The `expect` / `assert` builtin must build its failure message with
/// `crate::testing::assertions`, not with a private copy of the wording. The
/// `expect` statement and the bytecode VM already route through that library;
/// a private duplicate here is how the three drift apart.
///
/// The parser turns `expect a to be b` into the `Expect` statement, so this arm
/// is reached through `builtin` directly rather than through a `.rb` file.
#[cfg(test)]
mod expect_builtin_tests {
    use super::*;
    use crate::testing::assertions::assert_values_equal;

    fn call(name: &str, args: &[Value]) -> Result<Option<Value>> {
        builtin(Span::new(7, 5), name, args)
    }

    #[test]
    fn the_builtin_prints_the_message_the_assertion_library_builds() {
        for name in ["expect", "assert"] {
            let error = call(name, &[Value::Number(1.0), Value::Number(2.0)])
                .expect_err("1 is not 2, so the assertion must fail");

            let built = assert_values_equal(&Value::Number(2.0), &Value::Number(1.0))
                .expect_err("1 is not 2")
                .to_string();

            match error {
                Error::Runtime(message, span) => {
                    assert_eq!(
                        message, built,
                        "`{}` must report the failure the library builds",
                        name
                    );
                    assert_eq!(
                        span,
                        Span::new(7, 5),
                        "the failure must be reported against the call site"
                    );
                }
                other => panic!("`{}` must fail with a Runtime error, got {:?}", name, other),
            }
        }
    }

    #[test]
    fn the_builtin_accepts_a_matching_pair() {
        for name in ["expect", "assert"] {
            let value = call(
                name,
                &[Value::Text("a".to_string()), Value::Text("a".to_string())],
            )
            .unwrap_or_else(|e| panic!("`{}` must accept equal values, got {}", name, e));
            assert!(
                matches!(value, Some(Value::Nothing)),
                "`{}` returns nothing, got {:?}",
                name,
                value
            );
        }
    }

    /// The edge cases the argument list can present. Each must be a clean
    /// `Runtime` error naming the problem — never a panic, and never a silent
    /// pass that hides a broken assertion.
    #[test]
    fn edge_the_builtin_reports_its_own_argument_and_type_edges() {
        let too_few =
            call("expect", &[Value::Number(1.0)]).expect_err("one argument is not a pair");
        assert!(
            matches!(&too_few, Error::Runtime(message, _) if message.contains("two arguments")),
            "a missing argument must be named, got {:?}",
            too_few
        );

        // Pairs the assertion must accept.
        for (actual, expected, what) in [
            (Value::Nothing, Value::Nothing, "nothing"),
            (Value::Number(0.0), Value::Number(-0.0), "signed zero"),
            (
                Value::Text(String::new()),
                Value::Text(String::new()),
                "empty text",
            ),
            (Value::list(vec![]), Value::list(vec![]), "empty list"),
        ] {
            assert!(
                call("expect", &[actual, expected]).is_ok(),
                "{} compares equal to itself",
                what
            );
        }

        // Pairs it must refuse, each with a named failure.
        for (actual, expected, what) in [
            (
                Value::list(vec![]),
                Value::Text(String::new()),
                "empty list",
            ),
            (
                Value::Text(String::new()),
                Value::list(vec![]),
                "empty text",
            ),
            (Value::YesNo(true), Value::Number(1.0), "yes/no against one"),
            (Value::Nothing, Value::Number(0.0), "nothing against zero"),
            (
                Value::list(vec![]),
                Value::list(vec![Value::Number(1.0)]),
                "singleton list",
            ),
        ] {
            let failure = call("expect", &[actual.clone(), expected.clone()])
                .expect_err(&format!("{} must be refused", what));
            assert!(
                matches!(&failure, Error::Runtime(message, _) if !message.is_empty()),
                "{} must produce a named failure, got {:?}",
                what,
                failure
            );
        }
    }
}
