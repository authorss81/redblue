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

/// Every function that is not a user-defined one, in the one place both VMs
/// reach for.
///
/// `Ok(None)` says the name is not a builtin, which is how the caller knows to
/// look for a user declaration. This lives here rather than in either VM so
/// that a bytecode `CALL` and a tree-walked call cannot drift apart: there is
/// one implementation and both answer from it.
pub fn builtin(span: Span, name: &str, args: &[Value]) -> Result<Option<Value>> {
    // A call with the wrong number of arguments is refused before it runs, so
    // `files.read("a", "b")` is an error instead of a read of the first
    // argument alone, and `files.write("a")` is the same error rather than a
    // write with no content. The count is compared with `!=` and not with `>` so
    // that this refusal is the one `stdlib::call_module_function` makes for a
    // module spelling too: a bare `length()` and a `text.length()` are refused
    // in the same words, and a program cannot tell which spelling it wrote. The
    // function is named the way the module spells it, which is how every
    // message below names it too. A name that is not one of these functions has
    // no count to check, and is still the unknown function it is.
    //
    // Every name `arity` states is checked, not only the ones a module owns, so
    // a bare builtin with a fixed count is refused as firmly as the module
    // spelling of the same function.
    if let Some(expected) = crate::stdlib::arity(name) {
        if args.len() != expected {
            return Err(Error::Runtime(
                format!(
                    "{} takes {expected} argument(s), given {}",
                    crate::stdlib::display_name(name),
                    args.len()
                ),
                span,
            ));
        }
    }
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
        // The old `random` name, unregistered and unreachable as a global. It
        // drew from the wall clock with an `unwrap()` on it, so it draws from
        // the generator like every other random built-in does.
        "random" => {
            let drawn = random_below(span, 1000)?;
            Ok(Some(Value::Number(drawn as f64)))
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
            Ok(Some(Value::List(lines)))
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
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| Error::Runtime(e.to_string(), span))?;
            let secs = now.as_secs();
            let nanos = now.subsec_nanos();
            let record = crate::value::Fields::from([
                ("seconds".to_string(), Value::Number(secs as f64)),
                ("nanoseconds".to_string(), Value::Number(nanos as f64)),
            ]);
            Ok(Some(Value::Record(record)))
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
            // Refused before the `Duration` is built, not by the panic that
            // building one used to raise: `Duration::from_secs_f64` panics on a
            // negative duration, on `NaN`, and on a value past the end of a
            // `u64` of seconds, and a panic in the interpreter thread aborts
            // the process instead of raising something a `try` can catch.
            let duration = sleep_duration(seconds).ok_or_else(|| {
                Error::Runtime(
                    format!(
                        "time.sleep cannot sleep for {seconds} seconds: a sleep is a number of \
                         seconds from 0 to {MAX_SLEEP_SECS}"
                    ),
                    span,
                )
            })?;
            std::thread::sleep(duration);
            Ok(Some(Value::Nothing))
        }
        "time_format" => {
            // The format is optional, so `arity` states no count for this one;
            // a third argument is still more than the function documents.
            if args.len() > 2 {
                return Err(Error::Runtime(
                    format!("time.format takes 1 or 2 argument(s), given {}", args.len()),
                    span,
                ));
            }
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
            // A timestamp is a count of seconds since the epoch, so it is a
            // whole number from zero. A negative one used to cast to `0` and
            // answer `1970` as though the program had asked for it, and a huge
            // one used to cast to `u64::MAX` and overflow the `SystemTime`
            // addition below — which panics, and a panic in the interpreter
            // thread aborts the process instead of raising something a `try`
            // can catch.
            let seconds = timestamp_seconds(timestamp).ok_or_else(|| {
                Error::Runtime(
                    format!(
                        "time.format cannot format {timestamp} as a timestamp: it must be a \
                         whole number of seconds from 0"
                    ),
                    span,
                )
            })?;
            let datetime = chrono::DateTime::from_timestamp(seconds, 0).ok_or_else(|| {
                Error::Runtime(
                    format!("time.format cannot format {timestamp} as a date"),
                    span,
                )
            })?;
            Ok(Some(Value::Text(datetime.format(&format).to_string())))
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
            let parsed =
                chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S").map_err(|_| {
                    Error::Runtime(
                        "Invalid date format, use YYYY-MM-DD HH:MM:SS".to_string(),
                        span,
                    )
                })?;
            Ok(Some(Value::Number(parsed.and_utc().timestamp() as f64)))
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
            if actual != expected {
                return Err(Error::Runtime(
                    format!(
                        "Assertion failed: expected {:?} but got {:?}",
                        expected, actual
                    ),
                    span,
                ));
            }
            Ok(Some(Value::Nothing))
        }
        // Console module
        "console_log" => {
            match args {
                [arg] => println!("{}", arg),
                [] => {
                    return Err(Error::Runtime(
                        "console.log requires a value to print".to_string(),
                        span,
                    ))
                }
                _ => {
                    return Err(Error::Runtime(
                        "console.log takes one value to print".to_string(),
                        span,
                    ))
                }
            }
            Ok(Some(Value::Nothing))
        }
        "console_error" => {
            match args {
                [arg] => eprintln!("{}", arg),
                [] => {
                    return Err(Error::Runtime(
                        "console.error requires a value to print".to_string(),
                        span,
                    ))
                }
                _ => {
                    return Err(Error::Runtime(
                        "console.error takes one value to print".to_string(),
                        span,
                    ))
                }
            }
            Ok(Some(Value::Nothing))
        }
        "console_clear" => {
            print!("\x1B[2J\x1B[1H");
            Ok(Some(Value::Nothing))
        }
        // Text module. `join` is answered by `stdlib::builtin_function`, which
        // is the one place the list itself is joined; this arm is the refusal
        // that gets there first.
        //
        // A list of text, or an error. `text.join([1, yes, [1]], ",")` used to
        // stringify every element and answer `"1,yes,[1]"`, which makes `join`
        // the one function of the set that answers for a list it was not given:
        // `text.uppercase(1)` and `list.length(5)` both refuse, and a wrong
        // argument that produces plausible output is the worse of the two
        // failures, because a program with the bug in it goes on running. The
        // refusal names the element and what it is, which is the one thing the
        // caller can act on. `Ok(None)` for a list that is fine — the join is
        // the next thing that happens to it.
        "join" => {
            if let Some(Value::List(items)) = args.first() {
                for (at, item) in items.iter().enumerate() {
                    if !matches!(item, Value::Text(_)) {
                        return Err(Error::Runtime(
                            format!(
                                "join requires a list of text values, but element {} is {}",
                                at + 1,
                                item.type_name()
                            ),
                            span,
                        ));
                    }
                }
            }
            Ok(None)
        }
        // Random module
        "random_number" => {
            // One number draws from `0` to it, two draw from one to the other,
            // and anything else is refused by name: a wrong argument must not
            // answer a number that looks like a draw.
            let (min, max) = match (args.first(), args.get(1)) {
                (Some(Value::Number(min)), Some(Value::Number(max))) if args.len() == 2 => {
                    (*min, *max)
                }
                (Some(Value::Number(max)), None) if args.len() == 1 => (0.0, *max),
                _ => {
                    return Err(Error::Runtime(
                        "random_number requires one or two numbers".to_string(),
                        span,
                    ))
                }
            };
            let r = random_unit(span)?;
            // `min + r * (max - min)` overflows for a range as ordinary as
            // `-1e308` to `1e308`, because `max - min` is `infinity` — so a
            // perfectly drawable range could only ever fail, and
            // `Value::number`'s refusal names no function at all. Scaling each
            // end by a number in `[0, 1]` before adding cannot overflow on its
            // own, which leaves only the addition itself, and that is checked
            // and refused by the name of the function that was asked.
            let drawn = min * (1.0 - r) + max * r;
            if !drawn.is_finite() {
                return Err(Error::Runtime(
                    format!(
                        "random_number cannot draw from that range: the draw is {}",
                        crate::value::non_finite_name(drawn)
                    ),
                    span,
                ));
            }
            Ok(Some(Value::Number(drawn)))
        }
        "random_seed" => {
            // The generator is a stream, not a function of the clock, so a seed
            // is what makes a run repeatable: two programs that seed alike draw
            // alike, which is what lets a test assert a draw and what makes a
            // bug that depends on randomness reproducible.
            let seed = match args.first() {
                Some(Value::Number(n)) => *n,
                _ => {
                    return Err(Error::Runtime(
                        "random_seed requires a number".to_string(),
                        span,
                    ))
                }
            };
            RANDOM.with(|state| state.set(Random::seeded(seed_state(seed))));
            Ok(Some(Value::Nothing))
        }
        "random_choice" => {
            if let Some(Value::List(items)) = args.first() {
                if items.is_empty() {
                    return Ok(Some(Value::Nothing));
                }
                let index = random_below(span, items.len() as u64)? as usize;
                Ok(Some(items[index].clone()))
            } else {
                Err(Error::Runtime(
                    "random_choice requires a list".to_string(),
                    span,
                ))
            }
        }
        "random_shuffle" => {
            if let Some(Value::List(mut items)) = args.first().cloned() {
                // Fisher-Yates from the end, one draw per step. The old code
                // took a single draw and reused it for every step, so the
                // permutation depended on the *length* of the list and not on
                // its contents: `[1, 2]` always came back the other way round.
                for i in (1..items.len()).rev() {
                    let j = random_below(span, (i + 1) as u64)? as usize;
                    items.swap(i, j);
                }
                Ok(Some(Value::List(items)))
            } else {
                Err(Error::Runtime(
                    "random_shuffle requires a list".to_string(),
                    span,
                ))
            }
        }
        // Type conversion
        "type_of" => match args {
            [value] => Ok(Some(Value::Text(value.type_name().to_string()))),
            // The count is stated in `stdlib::arity` and refused by the gate at
            // the top of this function, so neither arm is reached through a call
            // today. They are here because the old
            // `args.first().map(..).unwrap_or("nothing")` answered `type_of()`
            // with the very type name `type_of(nothing)` gives, so a call that
            // forgot its argument was indistinguishable from a correct one — the
            // same failure as a `join` that stringified its list.
            given => Err(Error::Runtime(
                format!("type_of takes 1 argument(s), given {}", given.len()),
                span,
            )),
        },
        _ => Ok(None),
    }
}

/// The longest `time.sleep` accepts, in seconds: one year.
///
/// `Duration` holds whole seconds in a `u64`, so a value past the end of one
/// cannot become a duration at all — `Duration::from_secs_f64` *panics* on it,
/// and on a negative one and on `NaN`. A panic in the interpreter thread aborts
/// the process rather than raising something a `try` can catch, so all three are
/// refused instead. One year is inside what a duration can hold and outside what
/// a wait can mean: a program that asks to sleep longer is not going to see it
/// end either way, and saying so beats stopping the machine for the century.
pub const MAX_SLEEP_SECS: f64 = 365.0 * 24.0 * 60.0 * 60.0;

/// The `Duration` a sleep of `seconds` means, or `None` when it does not mean
/// one.
///
/// Built from whole seconds and a nanosecond remainder rather than through
/// `Duration::from_secs_f64`, which panics on everything [`sleep_duration`] is
/// asked to refuse. Both parts are inside their range once `seconds` is inside
/// `[0, MAX_SLEEP_SECS]`, so neither cast saturates.
fn sleep_duration(seconds: f64) -> Option<Duration> {
    if !seconds.is_finite() || !(0.0..=MAX_SLEEP_SECS).contains(&seconds) {
        return None;
    }
    let mut whole = seconds.trunc();
    let mut nanos = ((seconds - whole) * 1_000_000_000.0).round();
    // A remainder that rounds up to a whole second is carried into the seconds,
    // which keeps both halves inside the range `Duration::new` takes.
    if nanos >= 1_000_000_000.0 {
        whole += 1.0;
        nanos = 0.0;
    }
    Some(Duration::new(whole as u64, nanos as u32))
}

/// The whole seconds a timestamp names, or `None` when the number does not name
/// one.
///
/// A timestamp is a count of seconds since the epoch, so it has to be a whole
/// number from zero. Both ends of that were open: `timestamp as u64` saturates,
/// so a negative timestamp became `0` and answered `1970` as though the program
/// had asked for the epoch, and a large one became `u64::MAX` and overflowed the
/// `SystemTime` it was added to.
fn timestamp_seconds(timestamp: f64) -> Option<i64> {
    if !timestamp.is_finite()
        || timestamp < 0.0
        || timestamp.fract() != 0.0
        || timestamp >= i64::MAX as f64
    {
        return None;
    }
    Some(timestamp as i64)
}

/// The generator `math.random`, `random_choice` and `random_shuffle` draw from,
/// one per thread — so two VMs in one process do not share a stream, and a
/// program run on the interpreter thread starts from the state its own thread
/// left behind.
///
/// A generator rather than the wall clock, because the clock was the draw: two
/// calls inside one microsecond answered the *same* number, `as_nanos() % n` is
/// not uniform over the residues it can take, and a machine whose clock is set
/// before 1970 aborted the process at `duration_since(UNIX_EPOCH).unwrap()`.
/// The state advances on every draw and `math.seed` makes a run repeatable.
///
/// `SplitMix64`, from Steele, Lea and Flood: three lines, no dependency, and it
/// has no weak state — a plain `xorshift` is stuck forever if it is ever handed a
/// zero, which is exactly the value a seed of `0` would give.
#[derive(Clone, Copy)]
struct Random {
    state: u64,
    /// Whether a seed has been set. The clock seeds the generator the first time
    /// a draw is needed, and not before, so a program that seeds and never draws
    /// is not affected by the clock at all.
    seeded: bool,
}

impl Random {
    /// The odd increment of the sequence, and the two multipliers that mix it —
    /// the constants of `SplitMix64`, chosen so that every state advances and
    /// the bits of the result do not depend on the seed in a visible way.
    const STEP: u64 = 0xBF58_476D_1CE4_E5B9;
    const MIX_A: u64 = 0x94D0_49BB_1331_11EB;
    const MIX_B: u64 = 0xBF58_476D_1CE4_E5B9;

    fn unseeded() -> Self {
        Random {
            state: 0,
            seeded: false,
        }
    }

    fn seeded(state: u64) -> Self {
        Random {
            state,
            seeded: true,
        }
    }

    /// The next word of the sequence.
    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(Self::STEP);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(Self::MIX_A);
        z = (z ^ (z >> 27)).wrapping_mul(Self::MIX_B);
        z ^ (z >> 31)
    }

    /// A draw in `0..bound`, and `0` for a bound of zero.
    ///
    /// The low `2^k` bits, where `2^k` is the smallest power of two at or above
    /// the bound, are used: they are the ones `SplitMix64` mixes least, and
    /// taking a modulo of the whole word instead would make the low residues of a
    /// range one draw more likely than the high ones — a visible skew on a list
    /// of two.
    ///
    /// `2^k` need not be a multiple of the bound, so one value in it is
    /// over-represented; taking it again rather than letting it stand keeps every
    /// element of the range equally likely. Fewer than half of the values in
    /// `2^k` are above the bound, so this settles in a draw or two.
    ///
    /// A `loop` and not a call to itself. The rejection rate is under a half, so
    /// a hundred draws deep is a streak of a hundred consecutive heads — rare, and
    /// not something a test can provoke on purpose — but *every* draw of
    /// `math.random`, `random_choice` and `random_shuffle` reaches this function,
    /// and the recursion put one stack frame per rejected draw on the
    /// interpreter thread. A stack that runs out aborts the process instead of
    /// raising something a `try` can catch, so the retry costs no stack at all.
    fn below(&mut self, bound: u64) -> u64 {
        if bound == 0 {
            return 0;
        }
        let mask = low_bits(u64::BITS - (bound - 1).leading_zeros());
        loop {
            let draw = self.next_u64() & mask;
            if draw < bound {
                return draw;
            }
        }
    }
}

/// A mask over the low `bits` bits of a word — `0b0111` for three — and over all
/// 64 of them for a count that does not fit.
///
/// Written out rather than as `(1 << bits) - 1` because a bound with its top bit
/// set needs 64 of them, and shifting a `u64` left by 64 panics.
fn low_bits(bits: u32) -> u64 {
    if bits >= u64::BITS {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    }
}

thread_local! {
    static RANDOM: Cell<Random> = Cell::new(Random::unseeded());
}

/// The state `math.seed` sets for the generator.
///
/// The magnitude of the number when it is whole, and its fraction scaled up when
/// it is not — so `math.seed(1)` and `math.seed(1.5)` are both seeds rather than
/// one of them being a silent zero. The casts saturate, so a seed too large for
/// a `u64` is the largest one rather than a panic.
fn seed_state(seed: f64) -> u64 {
    let magnitude = seed.abs();
    if !magnitude.is_finite() {
        return 0;
    }
    if magnitude.fract() == 0.0 {
        magnitude as u64
    } else {
        (magnitude.fract() * crate::value::MAX_EXACT_INT) as u64
    }
}

/// One draw from this thread's generator, seeding it from the clock the first
/// time it is needed.
///
/// A clock that cannot say how long it has been — a machine whose time is set
/// before the epoch — is a `Runtime` error rather than a panic. The old code
/// `unwrap()`ed that `duration_since`, and a panic in the interpreter thread
/// aborts the process instead of raising something a `try` can catch. The state
/// is only stored once the draw has been taken from it, so a failure here leaves
/// the generator unseeded and the next call tries again.
fn with_random(span: Span, draw: impl FnOnce(&mut Random) -> u64) -> Result<u64> {
    RANDOM.with(|cell| {
        let mut generator = cell.get();
        if !generator.seeded {
            generator.state = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| {
                    Error::Runtime(
                        format!("Cannot read the clock to draw a random number: {e}"),
                        span,
                    )
                })?
                .as_nanos() as u64;
            generator.seeded = true;
        }
        let drawn = draw(&mut generator);
        cell.set(generator);
        Ok(drawn)
    })
}

/// A draw in `0.0..1.0`, from the top 53 bits of one word of the sequence.
///
/// 53 bits because that is what an `f64` holds exactly: a draw built from more
/// of them has neighbours no double can tell apart, so the low end of the range
/// would come out twice as often as the high end. The old draw used 20.
fn random_unit(span: Span) -> Result<f64> {
    let drawn = with_random(span, |generator| generator.next_u64() >> 11)?;
    Ok(drawn as f64 / (1u64 << 53) as f64)
}

/// A draw in `0..bound`, from this thread's generator.
fn random_below(span: Span, bound: u64) -> Result<u64> {
    with_random(span, |generator| generator.below(bound))
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
        .timeout(Duration::from_secs(NETWORK_TIMEOUT_SECS))
        .connect_timeout(Duration::from_secs(NETWORK_CONNECT_TIMEOUT_SECS))
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
                    rows.push(Value::List(std::mem::take(&mut row)));
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
        return Ok(Value::List(rows));
    }
    let cell = if was_quoted {
        field.clone()
    } else {
        field.trim().to_string()
    };
    row.push(Value::Text(cell));
    rows.push(Value::List(row));
    Ok(Value::List(rows))
}

fn parse_json_object(json: &str, span: Span) -> Result<Value> {
    let json = json.trim();
    if !json.starts_with('{') || !json.ends_with('}') {
        return Err(Error::Runtime("Invalid JSON object".to_string(), span));
    }
    let mut map = crate::value::Fields::new();
    let content = &json[1..json.len() - 1];
    if content.trim().is_empty() {
        return Ok(Value::Record(map));
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
    Ok(Value::Record(map))
}

fn parse_json_array(json: &str, span: Span) -> Result<Value> {
    let json = json.trim();
    if !json.starts_with('[') || !json.ends_with(']') {
        return Err(Error::Runtime("Invalid JSON array".to_string(), span));
    }
    let content = &json[1..json.len() - 1];
    if content.trim().is_empty() {
        return Ok(Value::List(Vec::new()));
    }
    let mut items = Vec::new();
    for item in split_json_elements(content) {
        items.push(parse_json(item, span)?);
    }
    Ok(Value::List(items))
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
    use std::collections::HashSet;

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

    /// The mask `Random::below` builds has to reach every value of the range and
    /// as few again as it can: reach too few and the draw is refused forever,
    /// reach too many and the low residues come out twice as often as the high
    /// ones.
    #[test]
    fn edge_the_mask_a_draw_is_taken_from_reaches_the_whole_range() {
        for bound in [
            1u64,
            2,
            3,
            4,
            5,
            7,
            8,
            9,
            255,
            256,
            257,
            1 << 32,
            (1 << 63) - 1,
            1 << 63,
            u64::MAX,
        ] {
            let bits = u64::BITS - (bound - 1).leading_zeros();
            let mask = low_bits(bits);
            let reach = mask as u128 + 1;
            assert!(
                reach >= bound as u128,
                "the mask for {bound} reaches only {reach} values"
            );
            assert!(
                reach - bound as u128 <= bound as u128,
                "{reach} values for a range of {bound} refuses at least half of them, \
                 so a draw would take more than two tries on average"
            );
        }
        assert_eq!(low_bits(0), 0, "no bits is no values");
        assert_eq!(low_bits(64), u64::MAX, "64 bits is every value");
    }

    /// `below` answers a value of the range from every seed, at every boundary
    /// — a bound of one and of two in particular, where a generator that mixed
    /// the low bits badly would show.
    #[test]
    fn edge_a_bounded_draw_answers_a_value_of_the_range_at_every_boundary() {
        let mut generator = Random::seeded(1);
        assert_eq!(generator.below(0), 0, "nothing to draw from nothing");
        for bound in [1u64, 2, 3, 4, 5, 8, 17, 1 << 40] {
            for seed in [0u64, 1, 2, u64::MAX] {
                let mut generator = Random::seeded(seed);
                for _ in 0..1_000 {
                    assert!(
                        generator.below(bound) < bound,
                        "seed {seed} drew outside 0..{bound}"
                    );
                }
            }
        }
        // Both halves of a range of two are reachable from one seed: a generator
        // that only ever set the low bit would answer `1` for all of them.
        let mut generator = Random::seeded(1);
        assert!(
            (0..100).any(|_| generator.below(2) == 0) && (0..100).any(|_| generator.below(2) == 1),
            "a range of two must be reachable on both sides"
        );
    }

    /// The sequence advances on every draw and does not depend on the seed being
    /// non-zero — a plain `xorshift` seeded with `0` is stuck forever, which is
    /// exactly the value `math.seed(0)` gives.
    #[test]
    fn edge_the_sequence_advances_from_every_seed() {
        for seed in [0u64, 1, 2, 42, u64::MAX] {
            let mut generator = Random::seeded(seed);
            let words: Vec<u64> = (0..1_000).map(|_| generator.next_u64()).collect();
            assert!(
                words.windows(2).all(|pair| pair[0] != pair[1]),
                "seed {seed} drew the same word twice in a row"
            );
            assert!(
                words.iter().collect::<HashSet<_>>().len() > 990,
                "seed {seed} repeated inside a thousand words"
            );
        }
        assert_ne!(
            Random::seeded(1).next_u64(),
            Random::seeded(2).next_u64(),
            "one word should not be the same for every seed"
        );
    }
}
