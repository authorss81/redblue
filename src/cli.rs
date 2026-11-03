//! Declared command-line flags, parsed from the arguments a program was given.
//!
//! `sys.argv()` answers the raw arguments, so every command-line program used to
//! hand-roll the same loop: find a flag, check the next element exists, convert
//! it, complain if it does not, and print usage text that drifts from what the
//! code accepts. This module is the declaration that replaces that loop.
//!
//! A declaration is a list of flag records:
//!
//! ```redblue
//! set options to [
//!     {name: "output", type: "text", default: "out.txt", help: "where to write"},
//!     {name: "count", type: "number", required: yes},
//!     {name: "verbose", type: "yes/no"}
//! ]
//! ```
//!
//! `sys.parse(options)` reads `sys.argv()` and answers a record holding one
//! field per flag, plus `arguments` (the positionals) and `help`/`usage` (the
//! generated help text). A flag's `type` is `text`, `number`, `yes/no` or
//! `path`; `takes_value` defaults to no for a `yes/no` flag and yes for every
//! other type. A declaration error, an unknown flag, a missing required flag and
//! a value of the wrong type are all catchable `Runtime` failures naming the
//! flag.

use std::collections::{HashMap, HashSet};

use crate::error::{Error, Result, Span};
use crate::value::{Fields, Value};

/// The field holding the positional arguments a parse did not consume.
pub const ARGUMENTS_FIELD: &str = "arguments";

/// The field saying whether `--help` was asked for.
pub const HELP_FIELD: &str = "help";

/// The field holding the usage text generated from the declaration.
pub const USAGE_FIELD: &str = "usage";

/// The type a flag's value has.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Text,
    Number,
    YesNo,
    Path,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Kind::Text => "text",
            Kind::Number => "number",
            Kind::YesNo => "yes/no",
            Kind::Path => "path",
        }
    }
}

/// One declared flag.
struct Flag {
    name: String,
    kind: Kind,
    takes_value: bool,
    default: Option<Value>,
    required: bool,
    help: String,
}

/// Parses `argv` against `declaration`.
///
/// `--help` and `-h` answer the generated usage without enforcing a required
/// flag: help is a request about the declaration, not a request to run with it.
pub fn parse(declaration: &Value, argv: &[String], span: Span) -> Result<Value> {
    let flags = read_declaration(declaration, span)?;
    let usage = usage_text(&flags);

    let mut given: HashMap<String, Value> = HashMap::new();
    let mut positionals: Vec<String> = Vec::new();
    let mut index = 0;

    while index < argv.len() {
        let token = &argv[index];

        if token == "--" {
            positionals.extend(argv[index + 1..].iter().cloned());
            break;
        }
        if token == "--help" || token == "-h" {
            return build_record(&flags, &given, positionals, true, &usage, span, false);
        }

        if let Some(rest) = token.strip_prefix("--") {
            let (name, inline) = match rest.split_once('=') {
                Some((name, value)) => (name, Some(value)),
                None => (rest, None),
            };
            let flag = flags
                .iter()
                .find(|flag| flag.name == name)
                .ok_or_else(|| Error::Runtime(format!("Unknown flag '--{name}'"), span))?;

            if flag.takes_value {
                let text = match inline {
                    Some(value) => value.to_string(),
                    None => {
                        index += 1;
                        let next = argv
                            .get(index)
                            .cloned()
                            .ok_or_else(|| missing_value(flag, span))?;
                        // Help is a request about the declaration, even when
                        // it appears where a value was expected: `--count
                        // --help` answers the generated usage rather than
                        // complaining that count is missing.
                        if next == "--help" || next == "-h" {
                            return build_record(
                                &flags,
                                &given,
                                positionals,
                                true,
                                &usage,
                                span,
                                false,
                            );
                        }
                        // A flag's value is the next token even when it starts
                        // with `-`, so a negative number still parses. What is
                        // not a value is another flag: `--count --verbose`
                        // should say the count is missing, not that
                        // `--verbose` is not a number.
                        if next == "--" || names_declared_flag(&next, &flags) {
                            return Err(missing_value(flag, span));
                        }
                        // An undeclared `--name` in value position is a typo,
                        // not a value: reporting it as one keeps a text flag
                        // from silently storing `--bogus` as its text.
                        if let Some(name) = flag_name(&next) {
                            return Err(Error::Runtime(format!("Unknown flag '--{name}'"), span));
                        }
                        next
                    }
                };
                let value = from_text(flag, &text, span)?;
                given.insert(flag.name.clone(), value);
            } else {
                if inline.is_some() {
                    return Err(Error::Runtime(
                        format!("Flag '--{}' does not take a value", flag.name),
                        span,
                    ));
                }
                given.insert(flag.name.clone(), Value::YesNo(true));
            }

            index += 1;
            continue;
        }

        if token.starts_with('-') && token != "-" && !is_negative_number(token) {
            return Err(Error::Runtime(format!("Unknown flag '{token}'"), span));
        }

        positionals.push(token.clone());
        index += 1;
    }

    build_record(&flags, &given, positionals, false, &usage, span, true)
}

/// Reads and validates a declaration.
fn read_declaration(declaration: &Value, span: Span) -> Result<Vec<Flag>> {
    let Value::List(items) = declaration else {
        return Err(Error::Runtime(
            format!(
                "sys.parse expects a list of flag declarations, but was given {}",
                declaration.type_name()
            ),
            span,
        ));
    };

    let mut flags = Vec::with_capacity(items.len());
    let mut names = HashSet::new();

    for (position, item) in items.iter().enumerate() {
        let Value::Record(fields) = item else {
            return Err(Error::Runtime(
                format!(
                    "flag declaration {} must be a record, but was given {}",
                    position + 1,
                    item.type_name()
                ),
                span,
            ));
        };

        let name = match fields.get("name") {
            Some(Value::Text(name)) => name.clone(),
            Some(other) => {
                return Err(Error::Runtime(
                    format!(
                        "flag declaration {} has a {} name where text belongs",
                        position + 1,
                        other.type_name()
                    ),
                    span,
                ))
            }
            None => {
                return Err(Error::Runtime(
                    format!("flag declaration {} has no name", position + 1),
                    span,
                ))
            }
        };
        validate_name(&name, span)?;
        if !names.insert(name.clone()) {
            return Err(Error::Runtime(
                format!("Flag '--{name}' is declared twice"),
                span,
            ));
        }

        let kind = match fields.get("type") {
            Some(Value::Text(kind)) => parse_kind(kind).ok_or_else(|| {
                Error::Runtime(
                    format!(
                        "Flag '--{name}' has unknown type '{kind}'; expected text, number, yes/no or path"
                    ),
                    span,
                )
            })?,
            Some(other) => {
                return Err(Error::Runtime(
                    format!(
                        "Flag '--{name}' has a {} type where text belongs",
                        other.type_name()
                    ),
                    span,
                ))
            }
            None => {
                return Err(Error::Runtime(
                    format!("Flag '--{name}' has no type"),
                    span,
                ))
            }
        };

        let takes_value = match fields.get("takes_value") {
            Some(Value::YesNo(takes)) => *takes,
            Some(other) => {
                return Err(Error::Runtime(
                    format!(
                        "Flag '--{name}' has a {} takes_value where yes/no belongs",
                        other.type_name()
                    ),
                    span,
                ))
            }
            None => kind != Kind::YesNo,
        };
        if !takes_value && kind != Kind::YesNo {
            return Err(Error::Runtime(
                format!(
                    "Flag '--{name}' is {} and takes no value, but only a yes/no flag may take no value",
                    kind.name()
                ),
                span,
            ));
        }

        let required = match fields.get("required") {
            Some(Value::YesNo(required)) => *required,
            Some(other) => {
                return Err(Error::Runtime(
                    format!(
                        "Flag '--{name}' has a {} required where yes/no belongs",
                        other.type_name()
                    ),
                    span,
                ))
            }
            None => false,
        };

        let help = match fields.get("help") {
            Some(Value::Text(help)) => help.clone(),
            Some(other) => {
                return Err(Error::Runtime(
                    format!(
                        "Flag '--{name}' has a {} help where text belongs",
                        other.type_name()
                    ),
                    span,
                ))
            }
            None => String::new(),
        };

        let default = match fields.get("default") {
            Some(Value::Nothing) | None => None,
            Some(value) => Some(from_default(&name, kind, value, span)?),
        };
        if required && default.is_some() {
            return Err(Error::Runtime(
                format!("Flag '--{name}' cannot be required and have a default"),
                span,
            ));
        }

        for key in fields.keys() {
            if !matches!(
                key.as_str(),
                "name" | "type" | "takes_value" | "default" | "required" | "help"
            ) {
                return Err(Error::Runtime(
                    format!("Flag '--{name}' has an unknown field '{key}'"),
                    span,
                ));
            }
        }

        flags.push(Flag {
            name,
            kind,
            takes_value,
            default,
            required,
            help,
        });
    }

    Ok(flags)
}

/// Refuses a flag name that cannot be written back as `--name`.
fn validate_name(name: &str, span: Span) -> Result<()> {
    if name.is_empty() {
        return Err(Error::Runtime(
            "a flag name cannot be empty".to_string(),
            span,
        ));
    }
    if name.starts_with('-') || name.contains('=') || name.chars().any(char::is_whitespace) {
        return Err(Error::Runtime(
            format!("'{name}' is not a usable flag name"),
            span,
        ));
    }
    if matches!(name, ARGUMENTS_FIELD | HELP_FIELD | USAGE_FIELD) {
        return Err(Error::Runtime(
            format!("'{name}' is a field sys.parse reserves, so it cannot name a flag"),
            span,
        ));
    }
    Ok(())
}

fn parse_kind(kind: &str) -> Option<Kind> {
    match kind {
        "text" => Some(Kind::Text),
        "number" => Some(Kind::Number),
        "yes/no" => Some(Kind::YesNo),
        "path" => Some(Kind::Path),
        _ => None,
    }
}

/// Converts the text a flag was given.
fn from_text(flag: &Flag, text: &str, span: Span) -> Result<Value> {
    match flag.kind {
        Kind::Text => Ok(Value::Text(text.to_string())),
        Kind::Number => {
            let number = text.trim().parse::<f64>().map_err(|_| {
                Error::Runtime(
                    format!(
                        "Flag '--{}' expects a number, but '{text}' is not a number",
                        flag.name
                    ),
                    span,
                )
            })?;
            let number = crate::value::finite_number(number, span).map_err(|_| {
                Error::Runtime(
                    format!(
                        "Flag '--{}' expects a number, but '{text}' is not a finite number",
                        flag.name
                    ),
                    span,
                )
            })?;
            Ok(Value::Number(number))
        }
        Kind::YesNo => match text.trim().to_ascii_lowercase().as_str() {
            "yes" | "true" => Ok(Value::YesNo(true)),
            "no" | "false" => Ok(Value::YesNo(false)),
            _ => Err(Error::Runtime(
                format!(
                    "Flag '--{}' expects yes/no, but '{text}' is neither",
                    flag.name
                ),
                span,
            )),
        },
        Kind::Path => Ok(crate::path::value(text)),
    }
}

/// Converts the value a declaration wrote as a default.
fn from_default(name: &str, kind: Kind, value: &Value, span: Span) -> Result<Value> {
    let converted = match kind {
        Kind::Text => match value {
            Value::Text(text) => Value::Text(text.clone()),
            _ => return Err(default_error(name, kind, value, span)),
        },
        Kind::Number => match value {
            Value::Number(number) => Value::Number(*number),
            _ => return Err(default_error(name, kind, value, span)),
        },
        Kind::YesNo => match value {
            Value::YesNo(answer) => Value::YesNo(*answer),
            _ => return Err(default_error(name, kind, value, span)),
        },
        Kind::Path => match crate::path::text_of(value) {
            Some(text) => crate::path::value(text),
            None => return Err(default_error(name, kind, value, span)),
        },
    };
    Ok(converted)
}

fn default_error(name: &str, kind: Kind, value: &Value, span: Span) -> Error {
    Error::Runtime(
        format!(
            "Flag '--{name}' has a {} default where {} belongs",
            value.type_name(),
            kind.name()
        ),
        span,
    )
}

/// The failure for a value-taking flag with no value after it.
fn missing_value(flag: &Flag, span: Span) -> Error {
    Error::Runtime(
        format!(
            "Flag '--{}' needs a value of type {}",
            flag.name,
            flag.kind.name()
        ),
        span,
    )
}

/// The name in a `--name` or `--name=value` token, if it has one.
fn flag_name(token: &str) -> Option<&str> {
    let rest = token.strip_prefix("--")?;
    Some(rest.split_once('=').map(|(name, _)| name).unwrap_or(rest))
}

/// Whether a token that starts with `-` is a negative number rather than a
/// short flag. A command-line tool may be given `-5` as a positional argument,
/// and refusing it as an unknown flag would make negative numbers unusable.
fn is_negative_number(token: &str) -> bool {
    token.parse::<f64>().is_ok_and(|number| number.is_finite())
}

/// Whether `token` is `--name` or `--name=value` for a flag in the declaration.
fn names_declared_flag(token: &str, flags: &[Flag]) -> bool {
    flag_name(token).is_some_and(|name| flags.iter().any(|flag| flag.name == name))
}

/// Builds the record a program receives.
fn build_record(
    flags: &[Flag],
    given: &HashMap<String, Value>,
    positionals: Vec<String>,
    help: bool,
    usage: &str,
    span: Span,
    enforce_required: bool,
) -> Result<Value> {
    let mut fields = Fields::new();

    for flag in flags {
        let value = if let Some(value) = given.get(&flag.name) {
            value.clone()
        } else if let Some(value) = &flag.default {
            value.clone()
        } else if flag.required && enforce_required {
            return Err(Error::Runtime(
                format!("Missing required flag '--{}'", flag.name),
                span,
            ));
        } else if flag.kind == Kind::YesNo {
            Value::YesNo(false)
        } else {
            Value::Nothing
        };
        fields.insert(flag.name.clone(), value);
    }

    fields.insert(
        ARGUMENTS_FIELD.to_string(),
        Value::list(positionals.into_iter().map(Value::Text).collect()),
    );
    fields.insert(HELP_FIELD.to_string(), Value::YesNo(help));
    fields.insert(USAGE_FIELD.to_string(), Value::Text(usage.to_string()));

    Ok(Value::record(fields))
}

/// The usage text generated from the declaration alone.
fn usage_text(flags: &[Flag]) -> String {
    let mut lines = vec![
        "Usage: [options] [arguments]".to_string(),
        String::new(),
        "Options:".to_string(),
    ];

    for flag in flags {
        let mut line = format!("  --{}", flag.name);
        if flag.takes_value {
            line.push_str(&format!(" <{}>", flag.kind.name()));
        }
        if flag.required {
            line.push_str(" (required)");
        }
        if let Some(default) = &flag.default {
            line.push_str(&format!(" (default: {default})"));
        }
        if !flag.help.is_empty() {
            line.push_str("  ");
            line.push_str(&flag.help);
        }
        lines.push(line);
    }

    lines.push("  --help, -h  show this help".to_string());
    lines.join("\n")
}
