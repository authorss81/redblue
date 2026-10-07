use crate::error::{Error, Result, Span};
use crate::value::Value;
use std::collections::HashMap;

/// The module names whose functions are registered as `module_function`
/// builtins, so that a caller can tell `json.parse` — a module — from a
/// variable that happens to be followed by a `.`.
///
/// Every name here is one SPEC.md §Standard Library or README.md documents, so
/// `tests/stdlib_module_docs_test.rs` fails if one of the two stops agreeing
/// with this list.
pub const MODULES: &[&str] = &[
    "console", "csv", "files", "formats", "json", "list", "math", "network", "text", "time",
];

/// The builtins a module call reaches, as the `module_function` name the call
/// compiles to and the builtin it is a second spelling of.
///
/// `text.uppercase("hi")` is the builtin `uppercase`, and the module name is
/// what a document spells it with. A member that is not listed here is not a
/// function of that module, and a call of it is refused by name rather than
/// answered as something else.
pub const MODULE_FUNCTIONS: &[(&str, &str)] = &[
    ("console_clear", "console_clear"),
    ("console_error", "console_error"),
    ("console_log", "console_log"),
    ("csv_parse", "csv_parse"),
    ("files_append", "files_append"),
    ("files_copy", "files_copy"),
    ("files_delete", "files_delete"),
    ("files_exists", "files_exists"),
    ("files_lines", "files_lines"),
    ("files_read", "files_read"),
    ("files_rename", "files_rename"),
    ("files_write", "files_write"),
    ("formats_parse_csv", "csv_parse"),
    ("formats_parse_json", "json_parse"),
    ("formats_to_json", "json_stringify"),
    ("json_parse", "json_parse"),
    ("json_stringify", "json_stringify"),
    ("list_length", "length"),
    ("math_abs", "abs"),
    ("math_ceil", "ceil"),
    ("math_floor", "floor"),
    ("math_random", "random_number"),
    ("math_round", "round"),
    ("math_seed", "random_seed"),
    ("math_sqrt", "sqrt"),
    ("network_get", "network_get"),
    ("network_post", "network_post"),
    ("text_join", "join"),
    ("text_length", "length"),
    ("text_lowercase", "lowercase"),
    ("text_split", "split"),
    ("text_trim", "trim"),
    ("text_uppercase", "uppercase"),
    ("time_format", "time_format"),
    ("time_now", "time_now"),
    ("time_sleep", "time_sleep"),
    ("time_unix", "time_unix"),
];

/// Whether `name` is one of the [`MODULE_FUNCTIONS`], which is what a receiver
/// naming a module resolves its member call against.
pub fn is_module_function(name: &str) -> bool {
    MODULE_FUNCTIONS
        .iter()
        .any(|(qualified, _)| *qualified == name)
}

/// Whether `name` is the builtin one of the [`MODULE_FUNCTIONS`] reaches —
/// `uppercase`, `files_read` — as opposed to a name registered for a function
/// that does not exist yet.
///
/// The argument count applies only to the first kind: a call that has too many
/// arguments is refused by the function it names, and a name that is not a
/// function has nothing to count.
pub fn is_module_builtin(name: &str) -> bool {
    MODULE_FUNCTIONS.iter().any(|(_, builtin)| *builtin == name)
}

/// The builtin the `module_function` `name` reaches, and the name the program
/// wrote it as — `("uppercase", "text.uppercase")` for `text_uppercase`.
pub fn resolve_module_function(name: &str) -> Option<(&'static str, String)> {
    MODULE_FUNCTIONS
        .iter()
        .find_map(|(qualified, builtin)| (*qualified == name).then(|| (*builtin, dotted(name))))
}

/// The name a program writes a builtin as: `text_uppercase` is
/// `text.uppercase`, and a bare `uppercase` is left alone.
pub fn dotted(name: &str) -> String {
    name.replacen('_', ".", 1)
}

/// The name a message about `builtin` should use.
///
/// [`dotted`] turns the *first* underscore into a dot, which is right for a name
/// a module owns — `files_read` is `files.read` — and wrong for one no module
/// owns: `type_of` is `type_of`, not `type.of`. A module owns a name when
/// [`MODULE_FUNCTIONS`] sends some `module_function` to it, so that is what
/// decides, and every builtin whose count [`arity`] states has an entry there
/// except `type_of`.
pub fn display_name(name: &str) -> String {
    if is_module_builtin(name) {
        dotted(name)
    } else {
        name.to_string()
    }
}

/// `message` with the builtin's own name replaced by the spelling the program
/// used, so a call written `formats.parse_json` is told about
/// `formats.parse_json` rather than about the `json.parse` it reaches, and a
/// call written `math.random` is told about `math.random` rather than about the
/// `random_number` it reaches. A message that names the builtin in neither
/// spelling is left alone.
pub fn named_as(message: &str, builtin: &str, written: &str) -> String {
    let own = dotted(builtin);
    if own == written {
        return message.to_string();
    }
    if let Some(at) = name_at(message, &own) {
        return replace_at(message, at, own.len(), written);
    }
    match name_at(message, builtin) {
        Some(at) => replace_at(message, at, builtin.len(), written),
        None => message.to_string(),
    }
}

/// The byte offset `name` starts at in `message`, when it is a whole word there:
/// a name is only the function's name when it is not part of a longer one, so
/// `length` is not found inside `lengthens`.
fn name_at(message: &str, name: &str) -> Option<usize> {
    let word = |c: char| c.is_alphanumeric() || c == '_';
    message.match_indices(name).find_map(|(at, _)| {
        let before = message[..at].chars().next_back();
        let after = message[at + name.len()..].chars().next();
        (!before.is_some_and(word) && !after.is_some_and(word)).then_some(at)
    })
}

/// `message` with the `from_len` bytes at `at` replaced by `written`.
fn replace_at(message: &str, at: usize, from_len: usize, written: &str) -> String {
    let mut out = String::with_capacity(message.len() + written.len());
    out.push_str(&message[..at]);
    out.push_str(written);
    out.push_str(&message[at + from_len..]);
    out
}

/// What a builtin needs, so that a call with the wrong arguments is told what
/// it should have said.
///
/// `None` means the builtin is not one this phase made reachable: a name
/// registered for a function that does not exist yet is still reported as an
/// unknown function, which is what it is.
pub fn requirement(builtin: &str) -> Option<&'static str> {
    Some(match builtin {
        "uppercase" | "lowercase" | "trim" => "a text argument",
        "split" => "a text argument and a separator",
        "join" => "a list and a separator",
        "length" => "a list or a text argument",
        "abs" | "floor" | "ceil" | "round" | "sqrt" => "a number argument",
        "console_log" | "console_error" => "one value to print",
        "random_number" => "one or two numbers",
        "random_seed" => "a number",
        "files_read" | "files_exists" | "files_lines" | "files_delete" => "a text path",
        "files_write" | "files_append" | "files_copy" | "files_rename" => "two text arguments",
        "time_sleep" => "a number",
        "time_unix" => "a text",
        "json_parse" | "csv_parse" => "text",
        "json_stringify" => "a value",
        "network_get" => "a URL",
        "network_post" => "a URL and a body",
        _ => return None,
    })
}

/// How many arguments a builtin takes, so that a call of the wrong count is
/// refused rather than answered from the arguments that happen to be there.
///
/// `None` means the arity is not one this phase states. It is also the answer
/// for a builtin whose count is not fixed — `math.random(5)` draws from zero
/// and `math.random(1, 5)` draws from one, so one number or two is what it
/// takes — and for a builtin whose last argument is optional, which is
/// `time.format(ts)` against `time.format(ts, format)`.
///
/// A count this phase does state is compared with `!=`, and is checked by both
/// callers: the module spelling refuses a call of the wrong count before the
/// builtin runs, and [`crate::runtime::builtin`] refuses it the same way, so a
/// bare name answers the same as the module spelling does.
///
/// `type_of` is here and has no module: it is the one builtin whose count is
/// fixed, whose name is bare, and whose name has an underscore in it. It used to
/// answer `type_of()` with the type name `type_of(nothing)` gives, so a call
/// that forgot its argument looked exactly like a correct one — see FINDINGS.md.
pub fn arity(builtin: &str) -> Option<usize> {
    match builtin {
        "uppercase" | "lowercase" | "trim" | "length" | "abs" | "floor" | "ceil" | "round"
        | "sqrt" => Some(1),
        "split" | "join" => Some(2),
        "console_log" | "console_error" => Some(1),
        "console_clear" => Some(0),
        "files_read" | "files_exists" | "files_lines" | "files_delete" => Some(1),
        "files_write" | "files_append" | "files_copy" | "files_rename" => Some(2),
        "time_now" => Some(0),
        "time_sleep" | "time_unix" => Some(1),
        "json_parse" | "json_stringify" | "csv_parse" => Some(1),
        "network_get" => Some(1),
        "network_post" => Some(2),
        "random_seed" | "type_of" => Some(1),
        _ => None,
    }
}

/// Calls the module function `name` with `args`, and is the one place a module
/// name is resolved: both VMs route here, so a `.rbc` cannot mean something
/// other than the source it was compiled from.
///
/// `None` means `name` is not a module function, which is what the caller
/// matched on before getting here.
pub fn call_module_function(span: Span, name: &str, args: &[Value]) -> Option<Result<Value>> {
    let (builtin, written) = resolve_module_function(name)?;
    // Too few or too many arguments is refused before the builtin runs, so a
    // call of one argument with two is an error rather than a call of the first
    // argument alone. `crate::runtime::builtin` refuses the same two counts in
    // the same words, which is what lets a bare `length()` and a `text.length()`
    // fail identically.
    if let (Some(expected), given) = (arity(builtin), args.len()) {
        if given != expected {
            return Some(Err(Error::Runtime(
                format!("{written} takes {expected} argument(s), given {given}"),
                span,
            )));
        }
    }
    // A builtin that refused is named the way the program spelled it: the call
    // is `formats.parse_json`, so that is what the error says.
    match crate::runtime::builtin(span, builtin, args) {
        Ok(Some(value)) => return Some(Ok(value)),
        Ok(None) => {}
        Err(error @ Error::Runtime(..)) => {
            return Some(Err(match error {
                Error::Runtime(message, span) => {
                    Error::Runtime(named_as(&message, builtin, &written), span)
                }
                other => other,
            }));
        }
        Err(other) => return Some(Err(other)),
    }
    Some(match builtin_function(builtin, args.to_vec()) {
        Some(value) => Ok(value),
        // A builtin with no requirement to state is one that does not exist, so
        // it is reported as the unknown function it is. Named the way the
        // program spelled it, for the same reason the requirement below is: the
        // `name` in `module_function` form is an encoding the program never
        // wrote and cannot read back out of a message.
        None => Err(Error::Runtime(
            requirement(builtin).map_or_else(
                || format!("Unknown function '{written}'"),
                |needs| format!("{written} requires {needs}"),
            ),
            span,
        )),
    })
}

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
    globals.insert("pop".to_string(), Value::Builtin("pop".to_string()));
    globals.insert("shift".to_string(), Value::Builtin("shift".to_string()));
    globals.insert("map".to_string(), Value::Builtin("map".to_string()));
    globals.insert("filter".to_string(), Value::Builtin("filter".to_string()));
    globals.insert("reduce".to_string(), Value::Builtin("reduce".to_string()));

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
        "random_seed".to_string(),
        Value::Builtin("random_seed".to_string()),
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

    // The module spellings of the builtins above, so `text.uppercase("hi")`
    // reaches the same builtin `text.uppercase` names. The bare name is a
    // second registration of the same builtin, not a second way to call it:
    // `Vm::call` routes a `Value::Builtin` through the module table only, so a
    // bare `uppercase("hi")` is still an unknown function.
    for (qualified, builtin) in MODULE_FUNCTIONS {
        globals.insert(
            (*qualified).to_string(),
            Value::Builtin((*builtin).to_string()),
        );
    }

    globals
}

/// Calls the builtin `name`.
///
/// `None` means "wrong argument type or wrong argument count", and
/// `Some(Value::Nothing)` means "the builtin ran and has no answer" — `sqrt` of
/// a negative number is the last case. It has no real answer, and handing back
/// `NaN` would put a value into `Value::Number` that no comparison and no
/// display can be trusted on.
pub fn builtin_function(name: &str, args: Vec<Value>) -> Option<Value> {
    match name {
        // Math functions. A number is the only thing any of them answers, so
        // anything else is `None` — "wrong argument" — rather than `nothing`,
        // which means the function ran and has no answer: `sqrt` of a negative
        // number is the last case.
        "abs" => match args.first()? {
            Value::Number(_) => Some(args[0].abs()),
            _ => None,
        },
        "floor" => match args.first()? {
            Value::Number(_) => Some(args[0].floor()),
            _ => None,
        },
        "ceil" => match args.first()? {
            Value::Number(_) => Some(args[0].ceil()),
            _ => None,
        },
        "round" => match args.first()? {
            Value::Number(_) => Some(args[0].round()),
            _ => None,
        },
        "sqrt" => match args.first()? {
            Value::Number(_) => Some(args[0].sqrt()),
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
        "length" => match args.first()? {
            Value::Text(s) => Some(Value::Number(s.len() as f64)),
            Value::List(items) => Some(Value::Number(items.len() as f64)),
            _ => None,
        },
        // `split` and `join` are the two text functions the documents spell as
        // `text.split(..)` and `text.join(..)`, so they are reachable by both
        // names. An empty separator splits into single characters, because
        // there is nothing in the text for it to separate.
        "split" => match (args.first()?, args.get(1)?) {
            (Value::Text(text), Value::Text(separator)) if !separator.is_empty() => {
                Some(Value::List(
                    text.split(separator.as_str())
                        .map(|part| Value::Text(part.to_string()))
                        .collect(),
                ))
            }
            (Value::Text(text), Value::Text(_)) => Some(Value::List(
                text.chars().map(|c| Value::Text(c.to_string())).collect(),
            )),
            _ => None,
        },
        // A list of *text*, or nothing: `join` writes text between the elements
        // and it does not turn them into text first. `item.to_string()` on a
        // number, a `yes/no` or a nested list would answer a plausible line for
        // a list the caller did not pass, and `crate::runtime::builtin` refuses
        // the same list by name before getting here — this is the same rule
        // behind that refusal, kept here so the two cannot disagree.
        "join" => match (args.first()?, args.get(1)?) {
            (Value::List(items), Value::Text(separator)) => {
                let texts: Option<Vec<&str>> = items
                    .iter()
                    .map(|item| match item {
                        Value::Text(text) => Some(text.as_str()),
                        _ => None,
                    })
                    .collect();
                texts.map(|texts| Value::Text(texts.join(separator.as_str())))
            }
            _ => None,
        },

        _ => None,
    }
}

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

    fn sqrt(&self) -> Value {
        match self {
            Value::Number(n) => match n.sqrt() {
                root if root.is_finite() => Value::Number(root),
                _ => Value::Nothing,
            },
            _ => Value::Nothing,
        }
    }
}
