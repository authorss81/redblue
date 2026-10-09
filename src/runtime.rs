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
            use std::time::{SystemTime, UNIX_EPOCH};
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| Error::Runtime(e.to_string(), span))?;
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
            use std::time::{SystemTime, UNIX_EPOCH};
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| Error::Runtime(e.to_string(), span))?;
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
            std::thread::sleep(std::time::Duration::from_secs_f64(seconds));
            Ok(Some(Value::Nothing))
        }
        "time_format" => {
            use std::time::UNIX_EPOCH;
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
            let datetime = UNIX_EPOCH + std::time::Duration::from_secs(timestamp as u64);
            let secs = datetime
                .duration_since(UNIX_EPOCH)
                .map_err(|e| Error::Runtime(e.to_string(), span))?
                .as_secs() as i64;
            let tm = chrono::DateTime::from_timestamp(secs, 0)
                .ok_or_else(|| Error::Runtime("Invalid timestamp".to_string(), span))?;
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
            use std::time::{SystemTime, UNIX_EPOCH};
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| Error::Runtime(e.to_string(), span))?;
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
                use std::time::{SystemTime, UNIX_EPOCH};
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|e| Error::Runtime(e.to_string(), span))?;
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
                use std::time::{SystemTime, UNIX_EPOCH};
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|e| Error::Runtime(e.to_string(), span))?;
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
}
