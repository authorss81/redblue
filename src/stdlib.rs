use crate::value::Value;
use std::collections::HashMap;

/// The module names whose functions are registered as `module_function`
/// builtins, so that a caller can tell `json.parse` — a module — from a
/// variable that happens to be followed by a `.`.
///
/// `text` and `math` are here because SPEC.md documents `text.uppercase` and
/// `math.sqrt`. They were missing, which made every dotted spelling of them fail
/// in the analyzer as `Unknown variable 'text'` — one step before the VM could
/// report a missing function, and naming the wrong thing.
///
/// `formats` is deliberately **not** here. SPEC.md § formats used to document
/// `formats.parse_json`, `formats.to_json` and `formats.parse_csv`, none of
/// which were ever names in the language; JSON and CSV are their own modules.
/// That section was corrected rather than the module invented, because adding
/// `formats` would make a misspelling resolve to a namespace and fail later and
/// less clearly than it does now.
pub const MODULES: &[&str] = &[
    "bytes", "console", "csv", "files", "json", "list", "math", "network", "sys", "text", "time",
];

/// Whether `name` is one of the [`MODULES`].
pub fn is_module(name: &str) -> bool {
    MODULES.contains(&name)
}

pub fn builtins() -> HashMap<String, Value> {
    let mut globals = HashMap::new();

    // Math constants
    globals.insert("PI".to_string(), Value::Number(std::f64::consts::PI));
    globals.insert("E".to_string(), Value::Number(std::f64::consts::E));

    // Math functions
    globals.insert("abs".to_string(), Value::Builtin("abs".to_string()));
    globals.insert("floor".to_string(), Value::Builtin("floor".to_string()));
    globals.insert("ceil".to_string(), Value::Builtin("ceil".to_string()));
    globals.insert("round".to_string(), Value::Builtin("round".to_string()));
    globals.insert("sqrt".to_string(), Value::Builtin("sqrt".to_string()));
    globals.insert("pow".to_string(), Value::Builtin("pow".to_string()));
    globals.insert("sin".to_string(), Value::Builtin("sin".to_string()));
    globals.insert("cos".to_string(), Value::Builtin("cos".to_string()));
    globals.insert("tan".to_string(), Value::Builtin("tan".to_string()));
    globals.insert("log".to_string(), Value::Builtin("log".to_string()));
    globals.insert("exp".to_string(), Value::Builtin("exp".to_string()));

    // Text functions
    globals.insert(
        "uppercase".to_string(),
        Value::Builtin("uppercase".to_string()),
    );
    globals.insert(
        "lowercase".to_string(),
        Value::Builtin("lowercase".to_string()),
    );
    globals.insert("trim".to_string(), Value::Builtin("trim".to_string()));
    globals.insert("split".to_string(), Value::Builtin("split".to_string()));
    globals.insert("join".to_string(), Value::Builtin("join".to_string()));
    globals.insert(
        "contains".to_string(),
        Value::Builtin("contains".to_string()),
    );
    globals.insert(
        "starts_with".to_string(),
        Value::Builtin("starts_with".to_string()),
    );
    globals.insert(
        "ends_with".to_string(),
        Value::Builtin("ends_with".to_string()),
    );
    globals.insert("replace".to_string(), Value::Builtin("replace".to_string()));

    // List functions
    globals.insert("length".to_string(), Value::Builtin("length".to_string()));
    globals.insert("push".to_string(), Value::Builtin("push".to_string()));
    globals.insert("append".to_string(), Value::Builtin("append".to_string()));
    globals.insert("pop".to_string(), Value::Builtin("pop".to_string()));
    globals.insert("shift".to_string(), Value::Builtin("shift".to_string()));
    globals.insert("map".to_string(), Value::Builtin("map".to_string()));
    globals.insert("filter".to_string(), Value::Builtin("filter".to_string()));
    globals.insert("reduce".to_string(), Value::Builtin("reduce".to_string()));
    // `list.map`, the module spelling SPEC.md § list writes for the same
    // builtin. It is a higher-order call, so the VM resolves it rather than
    // `runtime::builtin`.
    globals.insert(
        "list_map".to_string(),
        Value::Builtin("list_map".to_string()),
    );

    // Type checking
    globals.insert(
        "is_number".to_string(),
        Value::Builtin("is_number".to_string()),
    );
    globals.insert("is_text".to_string(), Value::Builtin("is_text".to_string()));
    globals.insert("is_list".to_string(), Value::Builtin("is_list".to_string()));
    globals.insert(
        "is_record".to_string(),
        Value::Builtin("is_record".to_string()),
    );

    // Conversion
    globals.insert("to_text".to_string(), Value::Builtin("to_text".to_string()));
    globals.insert(
        "to_number".to_string(),
        Value::Builtin("to_number".to_string()),
    );
    globals.insert("to_list".to_string(), Value::Builtin("to_list".to_string()));

    // Files module
    globals.insert(
        "files_read".to_string(),
        Value::Builtin("files_read".to_string()),
    );
    globals.insert(
        "files_write".to_string(),
        Value::Builtin("files_write".to_string()),
    );
    globals.insert(
        "files_append".to_string(),
        Value::Builtin("files_append".to_string()),
    );
    globals.insert(
        "files_exists".to_string(),
        Value::Builtin("files_exists".to_string()),
    );
    globals.insert(
        "files_lines".to_string(),
        Value::Builtin("files_lines".to_string()),
    );
    globals.insert(
        "files_delete".to_string(),
        Value::Builtin("files_delete".to_string()),
    );
    globals.insert(
        "files_copy".to_string(),
        Value::Builtin("files_copy".to_string()),
    );
    globals.insert(
        "files_rename".to_string(),
        Value::Builtin("files_rename".to_string()),
    );

    // Time module
    globals.insert(
        "time_now".to_string(),
        Value::Builtin("time_now".to_string()),
    );
    globals.insert(
        "time_sleep".to_string(),
        Value::Builtin("time_sleep".to_string()),
    );
    globals.insert(
        "time_format".to_string(),
        Value::Builtin("time_format".to_string()),
    );
    globals.insert(
        "time_unix".to_string(),
        Value::Builtin("time_unix".to_string()),
    );

    // Formats module
    globals.insert(
        "json_parse".to_string(),
        Value::Builtin("json_parse".to_string()),
    );
    globals.insert(
        "json_stringify".to_string(),
        Value::Builtin("json_stringify".to_string()),
    );
    globals.insert(
        "csv_parse".to_string(),
        Value::Builtin("csv_parse".to_string()),
    );

    // Network module
    globals.insert(
        "network_get".to_string(),
        Value::Builtin("network_get".to_string()),
    );
    globals.insert(
        "network_post".to_string(),
        Value::Builtin("network_post".to_string()),
    );

    // Testing module
    globals.insert("expect".to_string(), Value::Builtin("expect".to_string()));
    globals.insert("assert".to_string(), Value::Builtin("assert".to_string()));

    // Console module
    globals.insert(
        "console_log".to_string(),
        Value::Builtin("console_log".to_string()),
    );
    globals.insert(
        "console_error".to_string(),
        Value::Builtin("console_error".to_string()),
    );
    globals.insert(
        "console_clear".to_string(),
        Value::Builtin("console_clear".to_string()),
    );

    // Random module
    globals.insert(
        "random_number".to_string(),
        Value::Builtin("random_number".to_string()),
    );
    globals.insert(
        "random_choice".to_string(),
        Value::Builtin("random_choice".to_string()),
    );
    globals.insert(
        "random_shuffle".to_string(),
        Value::Builtin("random_shuffle".to_string()),
    );

    // Type conversion
    globals.insert("type_of".to_string(), Value::Builtin("type_of".to_string()));

    // Bytes module: the binary file API. `files` writes text, so without this a
    // program cannot write a byte it cannot spell — which is most of a bytecode
    // file.
    globals.insert(
        "bytes_from_text".to_string(),
        Value::Builtin("bytes_from_text".to_string()),
    );
    globals.insert(
        "bytes_write".to_string(),
        Value::Builtin("bytes_write".to_string()),
    );
    globals.insert(
        "bytes_text".to_string(),
        Value::Builtin("bytes_text".to_string()),
    );

    // Sys module
    globals.insert(
        "sys_argv".to_string(),
        Value::Builtin("sys_argv".to_string()),
    );

    globals
}

/// The non-function globals `builtins()` registers.
///
/// `PI` and `E` are values rather than callables, so `is_builtin` — which
/// answers for names a program can *call* — does not cover them, and the
/// analyzer needs to know they are in scope.
pub const GLOBAL_CONSTANTS: &[&str] = &["PI", "E"];

/// Whether `name` is one of the [`GLOBAL_CONSTANTS`].
pub fn is_global(name: &str) -> bool {
    GLOBAL_CONSTANTS.contains(&name)
}

/// Calls the builtin `name`, for the builtins [`runtime::builtin`] does not
/// answer.
///
/// This function used to be **dead code**: both VMs asked `runtime::builtin`,
/// which matched only the names it implemented, so `abs`, `floor`, `ceil`,
/// `round`, `sqrt`, `uppercase`, `lowercase` and `trim` — all registered in
/// [`builtins`], all in SPEC.md and README.md — answered `Unknown function` at
/// runtime. The only callers of this function were tests, and a test calling it
/// directly is green whether or not a program can reach it.
///
/// It is now reached, through [`builtin`], which is the single place a builtin
/// name is resolved and is called by both engines.
///
/// `None` means "wrong argument type or wrong argument count", and
/// `Some(Value::Nothing)` means "the builtin ran and has no answer" — `sqrt` of
/// a negative number is the last case. It has no real answer, and handing back
/// `NaN` would put a value into `Value::Number` that no comparison and no
/// display can be trusted on.
pub fn builtin_function(name: &str, args: Vec<Value>) -> Option<Value> {
    match name {
        // Math functions
        "abs" => Some(args.first()?.abs()),
        "floor" => Some(args.first()?.floor()),
        "ceil" => Some(args.first()?.ceil()),
        "round" => Some(args.first()?.round()),
        // `sqrt` of a negative answers `nothing`, which is the contract
        // `numeric_edge_test.rs` pins and what SPEC.md states: a negative has no
        // real root, and `nothing` is a value the language can test for, where
        // `NaN` would be a `Value::Number` no comparison and no display can be
        // trusted on. It is *not* an error — the caller asked a well-formed
        // question and the answer is that there is no such number.
        "sqrt" => match args.first()? {
            Value::Number(n) => {
                let root = n.sqrt();
                if root.is_finite() {
                    Some(Value::Number(root))
                } else {
                    Some(Value::Nothing)
                }
            }
            _ => None,
        },
        "pow" => match (args.first()?, args.get(1)?) {
            (Value::Number(base), Value::Number(exponent)) => {
                let result = base.powf(*exponent);
                // A negative base with a fractional exponent has no real answer,
                // and an overflow has none that is finite. Both are refused the
                // same way `sqrt` refuses a negative, rather than becoming a
                // non-finite number no comparison can be trusted on.
                if result.is_finite() {
                    Some(Value::Number(result))
                } else {
                    Some(Value::Nothing)
                }
            }
            _ => None,
        },
        "sin" => Some(args.first()?.unary("sin")),
        "cos" => Some(args.first()?.unary("cos")),
        "tan" => Some(args.first()?.unary("tan")),
        "log" => match args.first()? {
            Value::Number(n) if *n > 0.0 => Some(Value::Number(n.ln())),
            _ => Some(Value::Nothing),
        },
        "exp" => match args.first()? {
            Value::Number(n) => {
                let result = n.exp();
                if result.is_finite() {
                    Some(Value::Number(result))
                } else {
                    Some(Value::Nothing)
                }
            }
            _ => None,
        },

        // Text functions
        "uppercase" => {
            if let Value::Text(s) = args.first()? {
                Some(Value::Text(s.to_uppercase()))
            } else {
                None
            }
        }
        "lowercase" => {
            if let Value::Text(s) = args.first()? {
                Some(Value::Text(s.to_lowercase()))
            } else {
                None
            }
        }
        "trim" => {
            if let Value::Text(s) = args.first()? {
                Some(Value::Text(s.trim().to_string()))
            } else {
                None
            }
        }
        "split" => match (args.first()?, args.get(1)?) {
            (Value::Text(text), Value::Text(by)) => Some(Value::list(
                // An empty separator would make every position a boundary and
                // never advance, so it is the one input that has no answer here.
                // Redblue has no regex, so this is a literal separator.
                if by.is_empty() {
                    return Some(Value::Nothing);
                } else {
                    text.split(by.as_str())
                        .map(|part| Value::Text(part.to_string()))
                        .collect()
                },
            )),
            _ => None,
        },
        "join" => match (args.first()?, args.get(1)?) {
            (Value::List(items), Value::Text(by)) => Some(Value::Text(
                items
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<String>>()
                    .join(by),
            )),
            _ => None,
        },
        // `contains` has two meanings in this language and both are load-bearing.
        // `contains(text, needle)` is the text search; `contains(list, value)` is
        // membership. The self-hosted compiler uses the list form on every
        // `skip_newlines` dispatch, so narrowing this to text broke the fixed point —
        // which is what caught it.
        "contains" => match (args.first()?, args.get(1)?) {
            (Value::Text(text), Value::Text(needle)) => {
                Some(Value::YesNo(text.contains(needle.as_str())))
            }
            (Value::List(items), value) => Some(Value::YesNo(items.contains(value))),
            _ => None,
        },
        "starts_with" => match (args.first()?, args.get(1)?) {
            (Value::Text(text), Value::Text(prefix)) => {
                Some(Value::YesNo(text.starts_with(prefix.as_str())))
            }
            _ => None,
        },
        "ends_with" => match (args.first()?, args.get(1)?) {
            (Value::Text(text), Value::Text(suffix)) => {
                Some(Value::YesNo(text.ends_with(suffix.as_str())))
            }
            _ => None,
        },
        "replace" => match (args.first()?, args.get(1)?, args.get(2)?) {
            (Value::Text(text), Value::Text(from), Value::Text(to)) if !from.is_empty() => {
                Some(Value::Text(text.replace(from.as_str(), to.as_str())))
            }
            _ => None,
        },

        // Type predicates
        "is_number" => Some(Value::YesNo(matches!(args.first()?, Value::Number(_)))),
        "is_text" => Some(Value::YesNo(matches!(args.first()?, Value::Text(_)))),
        "is_list" => Some(Value::YesNo(matches!(args.first()?, Value::List(_)))),
        "is_record" => Some(Value::YesNo(matches!(args.first()?, Value::Record(_)))),

        "length" => match args.first()? {
            Value::Text(s) => Some(Value::Number(s.chars().count() as f64)),
            Value::List(items) => Some(Value::Number(items.len() as f64)),
            _ => None,
        },
        // `pop` and `shift` return the element they removed and leave the list they
        // were given as it was. Redblue's lists are copy-on-write, so a program
        // that wants the shorter list writes `set xs to pop(xs)` — and a
        // program that wants only the element gets only the element. An empty
        // list has nothing to remove, which is `Value::Nothing` rather than a
        // fabricated element.
        "pop" => match args.first()? {
            Value::List(items) => items.last().cloned(),
            _ => None,
        },
        "shift" => match args.first()? {
            Value::List(items) => items.first().cloned(),
            _ => None,
        },
        "to_list" => match args.first()? {
            Value::List(_) => Some(args[0].clone()),
            Value::Text(text) => Some(Value::list(
                text.split_whitespace()
                    .map(|word| Value::Text(word.to_string()))
                    .collect(),
            )),
            _ => None,
        },

        _ => None,
    }
}

/// The single place a builtin name is resolved.
///
/// `runtime::builtin` answers the builtins that can fail and need a span; this
/// answers the rest, which is what [`builtin_function`] implements. It is called
/// by both engines, so a builtin cannot answer in the tree-walker and be missing
/// from the bytecode VM.
///
/// `Ok(None)` means "no builtin of that name", which is what lets the caller
/// carry on to user functions and finally to `Unknown function`. A builtin that
/// *ran* and refused is `Err`, and it is never downgraded to "no such name":
/// `files.read` given a number it cannot read has to say so rather than claim
/// the function does not exist.
pub fn builtin(
    span: crate::error::Span,
    name: &str,
    args: &[Value],
) -> crate::error::Result<Option<Value>> {
    // A module member is answered under its bare builtin name, so `text.uppercase`
    // and `uppercase` are one function rather than two.
    let name = resolve(name);
    if let Some(value) = crate::runtime::builtin(span, name, args)? {
        return Ok(Some(value));
    }
    match builtin_function(name, args.to_vec()) {
        // `Value::Nothing` out of `builtin_function` means the builtin ran and
        // has no answer to give — `sqrt` of a negative, `log` of zero, an
        // overflowing `pow`. At the language boundary that is an error naming
        // the function, never a number and never a silent `nothing`: the reason
        // is that a `Value::Number` must never hold a value no comparison can
        // settle, and a program that asked a question the language cannot answer
        // is owed the name of what it asked.
        Some(Value::Nothing) => Err(crate::error::Error::Runtime(
            format!("{name} has no answer for these arguments"),
            span,
        )),
        Some(value) => Ok(Some(value)),
        // A builtin that recognised itself and could not use the arguments is
        // also an error by name. `Unknown function` is the message for a name
        // nothing implements, and applying it to a name the language documents
        // is how `uppercase("hi")` came to answer `Unknown function
        // 'uppercase'` for as long as it did.
        None if is_builtin(name) => Err(crate::error::Error::Runtime(
            format!("{name} was called with arguments it cannot use"),
            span,
        )),
        None => Ok(None),
    }
}

/// Whether `name` is a builtin this crate can answer, used to tell a missing
/// builtin from a missing user function.
pub fn is_builtin(name: &str) -> bool {
    BUILTIN_NAMES.contains(&name)
}

/// Resolves `module.member` to the builtin name behind it.
///
/// The internal encoding of a module function is `module_member` — that is what
/// `qualified_member` builds and what the global table registers, which is why
/// `json.parse` is stored as `json_parse`. The *documented* modules are spelled
/// differently from the ones this table was grown from: `text.uppercase` is
/// stored as nothing, so the dotted spelling reached the VM and found nothing.
///
/// This only unwraps a name the table does **not** already hold. That
/// qualification is load-bearing: `files.append` is registered as
/// `files_append`, and unwrapping it to the bare `append` sent a working
/// two-argument call to a builtin that needs a binding to write through, so
/// `files.append(path, "a")` was refused as arguments it cannot use. A name
/// the table holds is already the right name.
///
/// `None` means the name is not an unwrappable module member, so the caller
/// carries on with the name it was given.
pub fn module_member_name(qualified: &str) -> Option<&str> {
    if BUILTIN_NAMES.contains(&qualified) {
        return None;
    }
    let (module, member) = qualified.split_once('_')?;
    if MODULES.contains(&module) && BUILTIN_NAMES.contains(&member) {
        Some(member)
    } else {
        None
    }
}

/// The one name a builtin is answerable under, given the name the VM holds.
///
/// A module member resolves to its bare builtin; anything else is itself. Both
/// engines go through this, which is what keeps `text.uppercase` and
/// `uppercase` from being two different functions.
pub fn resolve(qualified: &str) -> &str {
    module_member_name(qualified).unwrap_or(qualified)
}

/// Every name [`builtins`] registers.
///
/// This is the list a registered-but-unreachable builtin shows up against, and
/// `tests/stdlib_module_docs_test.rs` walks the registry itself rather than
/// trusting this constant to stay in step.
pub const BUILTIN_NAMES: &[&str] = &[
    "abs",
    "floor",
    "ceil",
    "round",
    "sqrt",
    "pow",
    "sin",
    "cos",
    "tan",
    "log",
    "exp",
    "uppercase",
    "lowercase",
    "trim",
    "split",
    "join",
    "contains",
    "starts_with",
    "ends_with",
    "replace",
    "length",
    "push",
    "append",
    "pop",
    "shift",
    "map",
    "filter",
    "reduce",
    "list_map",
    "is_number",
    "is_text",
    "is_list",
    "is_record",
    "to_text",
    "to_number",
    "to_list",
    "type_of",
    "files_read",
    "files_write",
    "files_append",
    "files_exists",
    "files_lines",
    "files_delete",
    "files_copy",
    "files_rename",
    "time_now",
    "time_sleep",
    "time_format",
    "time_unix",
    "json_parse",
    "json_stringify",
    "csv_parse",
    "network_get",
    "network_post",
    "expect",
    "assert",
    "console_log",
    "console_error",
    "console_clear",
    "random_number",
    "random_choice",
    "random_shuffle",
    "bytes_from_text",
    "bytes_write",
    "bytes_text",
    "sys_argv",
];

impl Value {
    fn abs(&self) -> Value {
        match self {
            Value::Number(n) => Value::Number(n.abs()),
            _ => Value::Nothing,
        }
    }

    fn floor(&self) -> Value {
        match self {
            Value::Number(n) => Value::Number(n.floor()),
            _ => Value::Nothing,
        }
    }

    fn ceil(&self) -> Value {
        match self {
            Value::Number(n) => Value::Number(n.ceil()),
            _ => Value::Nothing,
        }
    }

    fn round(&self) -> Value {
        match self {
            Value::Number(n) => Value::Number(n.round()),
            _ => Value::Nothing,
        }
    }

    /// One of `sin`, `cos` or `tan`. A non-finite answer is `Value::Nothing`
    /// rather than a number no comparison can be trusted on, which is what
    /// `tan` of a pole has to be.
    fn unary(&self, what: &str) -> Value {
        match self {
            Value::Number(n) => {
                let result = match what {
                    "sin" => n.sin(),
                    "cos" => n.cos(),
                    _ => n.tan(),
                };
                if result.is_finite() {
                    Value::Number(result)
                } else {
                    Value::Nothing
                }
            }
            _ => Value::Nothing,
        }
    }
}
