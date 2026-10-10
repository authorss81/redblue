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

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::error::{Error, Result, Span};
use crate::lexer::Lexer;
use crate::parser::{BinaryOp, Expr, Program, Statement, UnaryOp};
use crate::value::Value;

/// Parses a module file into the program its top level declares.
///
/// The caller keeps the `Program` to remember that the module has already been
/// loaded, so a second `import` of the same name is a no-op rather than a second
/// binding of the same names. [`module_bindings`] reads the declarations out of
/// the same `Program`, so a module is read and parsed once however many names it
/// contributes.
pub fn module_program(path: &str) -> Result<Program> {
    let source = std::fs::read_to_string(path)
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
        _ => Err(Error::Runtime(
            "Cannot access property on non-object".to_string(),
            span,
        )),
    }
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
            _ => Err(Error::Runtime("Cannot add non-numbers".to_string(), span)),
        },
        BinaryOp::Sub => {
            if let (Value::Number(a), Value::Number(b)) = (left, right) {
                Value::number(a - b, span)
            } else {
                Err(Error::Runtime(
                    "Cannot subtract non-numbers".to_string(),
                    span,
                ))
            }
        }
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
        BinaryOp::Equal => Ok(Value::YesNo(left == right)),
        BinaryOp::NotEqual => Ok(Value::YesNo(left != right)),
        BinaryOp::Less => {
            if let (Value::Number(a), Value::Number(b)) = (left, right) {
                Ok(Value::YesNo(a < b))
            } else {
                Err(Error::Runtime(
                    "Cannot compare non-numbers".to_string(),
                    span,
                ))
            }
        }
        BinaryOp::LessEqual => {
            if let (Value::Number(a), Value::Number(b)) = (left, right) {
                Ok(Value::YesNo(a <= b))
            } else {
                Err(Error::Runtime(
                    "Cannot compare non-numbers".to_string(),
                    span,
                ))
            }
        }
        BinaryOp::Greater => {
            if let (Value::Number(a), Value::Number(b)) = (left, right) {
                Ok(Value::YesNo(a > b))
            } else {
                Err(Error::Runtime(
                    "Cannot compare non-numbers".to_string(),
                    span,
                ))
            }
        }
        BinaryOp::GreaterEqual => {
            if let (Value::Number(a), Value::Number(b)) = (left, right) {
                Ok(Value::YesNo(a >= b))
            } else {
                Err(Error::Runtime(
                    "Cannot compare non-numbers".to_string(),
                    span,
                ))
            }
        }
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

/// The whole seconds `timestamp` names, counted from the Unix epoch, for
/// `chrono` to format.
///
/// `SystemTime + Duration` panics when the addition overflows, and it does so
/// from inside `std`, past every `Result` in this crate — `time.format(1e300)`
/// aborted the process. The range is therefore checked here, and a timestamp
/// that is not finite, not whole, or past what a date can name is a `Runtime`
/// error. `timestamp as u64` is not usable for the conversion: it saturates, so
/// a negative timestamp would have quietly formatted as the epoch rather than
/// as 1969.
fn seconds_for_format(timestamp: f64, span: Span) -> Result<i64> {
    if !timestamp.is_finite() {
        return Err(Error::Runtime(
            format!("time.format requires a finite number of seconds, got {timestamp}"),
            span,
        ));
    }
    // A timestamp with a fractional part keeps the behaviour it always had:
    // `as` truncates toward zero, so `time.format(1.5)` is one second after the
    // epoch. Refusing it would reject a program that used to work.
    // `i64::MAX` seconds is far past the last instant a `SystemTime` can hold,
    // and `chrono` cannot format one either, so the range is bounded here
    // rather than by whichever of the two overflows first. A timestamp before
    // the epoch is *not* out of range: 1969 is a real year, and this function
    // hands the seconds back for `chrono` to format, so `time.format(-1)` is
    // 1969-12-31 rather than an error.
    const MAX_SECONDS: f64 = 253_402_300_800.0; // 10000-01-01T00:00:00Z
    if timestamp < -MAX_SECONDS || timestamp > MAX_SECONDS {
        return Err(Error::Runtime(
            format!("time.format cannot represent {timestamp} seconds from 1970-01-01T00:00:00Z"),
            span,
        ));
    }
    Ok(timestamp as i64)
}

/// The epoch seconds `time.unix` reports for a date `chrono` accepted.
///
/// `NaiveDateTime::parse_from_str` accepts a year as low as -262143, which
/// counts to about -8.3e12 seconds — a number `time.format` refuses, so the two
/// builtins disagreed about whether an instant existed. The parse itself is not
/// the panic risk this phase found elsewhere (`timestamp()` is i64 arithmetic
/// over a `chrono`-bounded year, so it cannot overflow), but the range is
/// checked here anyway so `time.unix` and `time.format` accept exactly the same
/// instants, and a date outside it is a catchable `Runtime` error naming
/// `time.unix` rather than a number no other builtin will take.
fn seconds_for_unix(text: &str, span: Span) -> Result<i64> {
    let parsed =
        chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S").map_err(|_| {
            Error::Runtime(
                "Invalid date format, use YYYY-MM-DD HH:MM:SS".to_string(),
                span,
            )
        })?;
    let seconds = parsed.and_utc().timestamp();
    // The same window `seconds_for_format` uses, so the two agree on both sides.
    const MAX_SECONDS: i64 = 253_402_300_800; // 10000-01-01T00:00:00Z
    if !(-MAX_SECONDS..=MAX_SECONDS).contains(&seconds) {
        return Err(Error::Runtime(
            format!("time.unix cannot represent {text} as seconds from 1970-01-01T00:00:00Z"),
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
                println!("{}", arg);
                Ok(Some(Value::Nothing))
            } else {
                Err(Error::Runtime("say requires an argument".to_string(), span))
            }
        }
        "length" | "len" => {
            if let Some(Value::List(items)) = args.first().cloned() {
                Ok(Some(Value::Number(items.len() as f64)))
            } else if let Some(Value::Text(s)) = args.first().cloned() {
                Ok(Some(Value::Number(s.len() as f64)))
            } else {
                Err(Error::Runtime(
                    "length requires a list or text".to_string(),
                    span,
                ))
            }
        }
        "input" | "ask" => {
            let mut input = String::new();
            if let Some(prompt) = args.first() {
                print!("{}", prompt);
            }
            std::io::stdin()
                .read_line(&mut input)
                .map_err(|e| Error::Runtime(e.to_string(), span))?;
            input.pop(); // Remove newline
            Ok(Some(Value::Text(input)))
        }
        "random" => {
            let now = since_epoch(SystemTime::now(), span)?;
            Ok(Some(Value::Number((now.as_nanos() % 1000) as f64)))
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
            std::fs::read_to_string(path)
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
            std::fs::write(path, content)
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
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .and_then(|mut f| std::io::Write::write_all(&mut f, content.as_bytes()))
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
            Ok(Some(Value::YesNo(std::path::Path::new(path).exists())))
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
            let content = std::fs::read_to_string(path)
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
            std::fs::remove_file(path)
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
            std::fs::copy(from, to)
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
            std::fs::rename(from, to)
                .map(|_| Some(Value::Nothing))
                .map_err(|e| Error::Io(format!("Failed to rename '{}' to '{}': {}", from, to, e)))
        }
        // Time module
        "time_now" => {
            let now = since_epoch(SystemTime::now(), span)?;
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
            std::thread::sleep(sleep_duration(seconds, span)?);
            Ok(Some(Value::Nothing))
        }
        "time_format" => {
            let (timestamp, format) = match (args.first(), args.get(1)) {
                (Some(Value::Number(ts)), Some(Value::Text(fmt))) => (*ts, fmt.clone()),
                (Some(Value::Number(ts)), None) => (*ts, "%Y-%m-%d %H:%M:%S".to_string()),
                _ => {
                    return Err(Error::Runtime(
                        "time.format requires a number and optional text".to_string(),
                        span,
                    ))
                }
            };
            let secs = seconds_for_format(timestamp, span)?;
            let tm = chrono::DateTime::from_timestamp(secs, 0).ok_or_else(|| {
                Error::Runtime(
                    format!("time.format cannot represent {timestamp} seconds from 1970-01-01T00:00:00Z"),
                    span,
                )
            })?;
            Ok(Some(Value::Text(tm.format(&format).to_string())))
        }
        "time_unix" => {
            let text = match args.first() {
                Some(Value::Text(s)) => s,
                _ => {
                    return Err(Error::Runtime(
                        "time.unix requires a text".to_string(),
                        span,
                    ))
                }
            };
            let seconds = seconds_for_unix(text, span)?;
            Ok(Some(Value::Number(seconds as f64)))
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
            Ok(Some(Value::Text(json_stringify(&value))))
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
            let client = network_client(span)?;
            let response = client
                .get(url)
                .send()
                .map_err(|e| Error::Runtime(format!("HTTP request failed: {}", e), span))?;
            let body = response
                .text()
                .map_err(|e| Error::Runtime(format!("Failed to read response: {}", e), span))?;
            Ok(Some(Value::Text(body)))
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
            let client = network_client(span)?;
            let response = client
                .post(url)
                .body(data.clone())
                .send()
                .map_err(|e| Error::Runtime(format!("HTTP request failed: {}", e), span))?;
            let body = response
                .text()
                .map_err(|e| Error::Runtime(format!("Failed to read response: {}", e), span))?;
            Ok(Some(Value::Text(body)))
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
                println!("{}", arg);
            }
            Ok(Some(Value::Nothing))
        }
        "console_error" => {
            if let Some(arg) = args.first() {
                eprintln!("{}", arg);
            }
            Ok(Some(Value::Nothing))
        }
        "console_clear" => {
            print!("\x1B[2J\x1B[1H");
            Ok(Some(Value::Nothing))
        }
        // Random module
        "random_number" => {
            let (min, max) = match (args.first(), args.get(1)) {
                (Some(Value::Number(min)), Some(Value::Number(max))) => (*min, *max),
                (Some(Value::Number(max)), None) => (0.0, *max),
                _ => (0.0, 1.0),
            };
            let now = since_epoch(SystemTime::now(), span)?;
            let r = (now.as_nanos() % 1000000) as f64 / 1000000.0;
            // `max - min` overflows for a range as ordinary as
            // `-1e308` to `1e308`, which is a number that does not exist.
            Value::number(min + r * (max - min), span).map(Some)
        }
        "random_choice" => {
            if let Some(Value::List(items)) = args.first() {
                if items.is_empty() {
                    return Ok(Some(Value::Nothing));
                }
                let now = since_epoch(SystemTime::now(), span)?;
                let idx = (now.as_nanos() as usize) % items.len();
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
                let now = since_epoch(SystemTime::now(), span)?;
                let seed = now.as_nanos() as usize;

                for i in (1..shuffled.len()).rev() {
                    let j = seed % (i + 1);
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
            std::fs::write(path, out)
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
fn network_client(span: Span) -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(NETWORK_TIMEOUT_SECS))
        .connect_timeout(std::time::Duration::from_secs(NETWORK_CONNECT_TIMEOUT_SECS))
        .build()
        .map_err(|e| Error::Runtime(format!("Cannot create the HTTP client: {}", e), span))
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
fn json_escape_text(text: &str) -> String {
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

fn json_stringify(value: &Value) -> String {
    match value {
        Value::Nothing => "null".to_string(),
        Value::YesNo(b) => {
            if *b {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        Value::Number(n) => {
            // JSON has no literal for a number that is not finite, so it is
            // written as `null`, which is what every JSON writer emits for one.
            // No Redblue program can reach this branch: see SPEC.md.
            if !n.is_finite() {
                "null".to_string()
            } else if n.fract() == 0.0 && n.abs() < 1e15 {
                format!("{}", *n as i64)
            } else {
                format!("{}", n)
            }
        }
        Value::Text(s) => json_escape_text(s),
        Value::List(items) => {
            let elements: Vec<String> = items.iter().map(json_stringify).collect();
            format!("[{}]", elements.join(", "))
        }
        Value::Record(fields) => {
            let pairs: Vec<String> = fields
                .iter()
                .map(|(k, v)| format!("{}: {}", json_escape_text(k), json_stringify(v)))
                .collect();
            format!("{{{}}}", pairs.join(", "))
        }
        Value::Function(_) => "null".to_string(),
        Value::Builtin(_) => "null".to_string(),
        Value::Object(_, _) => "null".to_string(),
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
                json_stringify(&Value::Number(value)),
                "null",
                "{} must be written as null, not as a JSON number",
                what
            );
        }
        // The widest finite numbers are numbers, and must not be nulled.
        for finite in [f64::MAX, f64::MIN, f64::MIN_POSITIVE, 0.0, -0.0, 1.5] {
            let written = json_stringify(&Value::Number(finite));
            assert_ne!(written, "null", "{} should still be written", finite);
        }
        assert_eq!(json_stringify(&Value::Number(42.0)), "42");
        assert_eq!(json_stringify(&Value::Number(1.5)), "1.5");
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
            match seconds_for_unix(text, span) {
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
        ] {
            let seconds = seconds_for_unix(text, Span::new(1, 1))
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
