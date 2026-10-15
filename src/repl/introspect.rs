//! Answers about *code*, rather than about the session.
//!
//! `:vars` and `:inspect` describe what a session has bound. Nothing described
//! what the language has: `:type` asked nothing about a value's kind and nothing
//! about where it came from, `:doc` named no module's members, `:which` could not
//! say whether a name was a builtin or something the user had typed, and a
//! builtin's shape lived only in the arms of `stdlib::builtin_function` and
//! `runtime::builtin`. Every one of those answers was a question about the
//! language, and all four meant leaving the REPL to get it.
//!
//! Everything here is pure: the VM-facing half lives in [`super::Repl`], which
//! gathers the facts (what the session has bound, where it bound it) and hands
//! them over. That is what lets the answer be asserted without a terminal.

use std::fmt;

use crate::lexer::Lexer;
use crate::stdlib;
use crate::value::Value;

/// Where a name comes from.
///
/// Ordered by the lookup a program would make, which is also the order
/// [`classify`] asks its questions in: what this session bound, then what the
/// registry holds, then a dotted member. A session `set` shadows a builtin of
/// the same name, and `.which` has to say so rather than report the builtin — the
/// binding is the one the next line will reach.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// A builtin the registry holds under this name.
    Builtin { name: String },
    /// A name this session bound, and where the line that bound it is.
    Session {
        name: String,
        definition: Definition,
    },
    /// A member of a standard library module, spelled `module.member`.
    StdlibMember { module: String, member: String },
    /// A member of a module or object this session built.
    SessionMember { module: String, member: String },
    /// Nothing binds this name.
    Unknown { name: String },
}

impl Origin {
    /// The one word naming which of these it is, for the first line of `.which`.
    pub fn kind(&self) -> &'static str {
        match self {
            Origin::Builtin { .. } => "builtin",
            Origin::Session { .. } => "session binding",
            Origin::StdlibMember { .. } => "module member",
            Origin::SessionMember { .. } => "session module member",
            Origin::Unknown { .. } => "unknown",
        }
    }
}

/// A place a name was written, as `file:line` plus the line itself.
///
/// `file` is a path for a name a file bound and `<session>` for one the user
/// typed, because a session line has no file and inventing one would point at
/// something that is not there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Definition {
    pub file: String,
    pub line: usize,
    pub source: String,
}

impl fmt::Display for Definition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.file, self.line)
    }
}

/// The file a name the user typed at the prompt is written in.
pub const SESSION_FILE: &str = "<session>";

/// Which of the [`Origin`]s `name` has.
///
/// `session_binding` is what the session holds under a bare `name`, and
/// `session_member` whether the `Module.member` it spells is one the session
/// built. Both are facts only the VM knows, so they are passed in rather than
/// looked up here.
pub fn classify(name: &str, session_binding: Option<&Definition>, session_member: bool) -> Origin {
    match name.split_once('.') {
        Some((module, member)) if !module.is_empty() && !member.is_empty() => {
            // A stdlib member first: `json.parse` is in the registry whether or
            // not this session has ever heard of it, and a session that declared
            // its own `json` would shadow it — which the caller reports by
            // answering `session_member` for a name the stdlib also has.
            if stdlib::module_has_member(module, member) && stdlib::member_resolves(module, member)
            {
                if session_member {
                    Origin::SessionMember {
                        module: module.to_string(),
                        member: member.to_string(),
                    }
                } else {
                    Origin::StdlibMember {
                        module: module.to_string(),
                        member: member.to_string(),
                    }
                }
            } else if session_member {
                Origin::SessionMember {
                    module: module.to_string(),
                    member: member.to_string(),
                }
            } else {
                Origin::Unknown {
                    name: name.to_string(),
                }
            }
        }
        _ => {
            // The session before the registry: `set uppercase to 5` binds
            // something the next line reaches under that name, and `.which`
            // reporting a builtin here would describe a name the program cannot
            // call any more.
            if let Some(definition) = session_binding {
                Origin::Session {
                    name: name.to_string(),
                    definition: definition.clone(),
                }
            } else if stdlib::is_builtin(name) {
                Origin::Builtin {
                    name: name.to_string(),
                }
            } else {
                Origin::Unknown {
                    name: name.to_string(),
                }
            }
        }
    }
}

/// The `.which` answer for `name`.
pub fn which_lines(name: &str, origin: &Origin) -> Vec<String> {
    let head = format!("{}: {}", name, origin.kind());
    match origin {
        Origin::Builtin { .. } | Origin::Unknown { .. } => vec![head],
        Origin::Session { definition, .. } => {
            let mut lines = vec![head, format!("  bound at  {}", definition)];
            // The line itself, when there is one to quote: a `file:line` the
            // user has to go and look up is half an answer, and for a session
            // line the history already holds the text.
            if !definition.source.is_empty() {
                lines.push(format!("  written as  {}", definition.source));
            }
            lines
        }
        Origin::StdlibMember { module, member } | Origin::SessionMember { module, member } => {
            let qualified = format!("{module}_{member}");
            vec![
                head,
                format!("  of module {}", module),
                format!("  callable as {}.{}", module, member),
                format!("  builtin    {}", stdlib::resolve(&qualified)),
            ]
        }
    }
}

/// The `.doc` answer for `module`, or `None` when it is not a module at all.
///
/// Every member is read out of [`stdlib::MODULE_MEMBERS`] and every shape out of
/// [`stdlib::builtin_signature`], so what this lists is what the registry holds.
/// A hand-written list here would be a fourth copy of the module surface, and
/// the one thing a user reads it to find out.
pub fn doc_lines(module: &str) -> Option<Vec<String>> {
    if !stdlib::is_module(module) {
        return None;
    }

    let mut lines = vec![format!("module {}:", module)];
    let members = stdlib::MODULE_MEMBERS
        .iter()
        .find(|(listed, _)| *listed == module)
        .map(|(_, members)| *members)
        .unwrap_or(&[]);

    if members.is_empty() {
        lines.push("  (no members: every function it has is a flat builtin)".to_string());
        return Some(lines);
    }

    let width = members
        .iter()
        .map(|member| member.len())
        .max()
        .unwrap_or(0)
        .max("member".len());

    for member in members {
        let qualified = format!("{module}_{member}");
        let shape = if stdlib::member_resolves(module, member) {
            match stdlib::builtin_signature(&qualified) {
                // The registry name is an implementation detail of the dotted
                // spelling — `json_parse` is how `json.parse` is stored, not what
                // the user types — so the prefix comes off before it is shown.
                Some(shape) => shape
                    .replacen(&format!("{module}_"), "", 1)
                    .replace('\n', "; "),
                None => "no shape is written down".to_string(),
            }
        } else {
            "no builtin answers this spelling".to_string()
        };
        lines.push(format!(
            "  {:width$}  {}",
            format!("{module}.{member}"),
            shape,
            width = width + module.len() + 1
        ));
    }

    Some(lines)
}

/// The `.sighelp` answer for `name`, or the refusal to give one.
///
/// `None` is a clean refusal rather than an empty listing, because "there is
/// nothing to say about this" and "here is what it takes" are different answers
/// and only one of them is a question the user can act on.
pub fn sighelp_lines(name: &str, session_function: Option<&Value>) -> Option<Vec<String>> {
    // The registry first, and the keyword check after it, because a name can be
    // both: `expect` is a builtin *and* the keyword `expect x to be y`, and a
    // keyword check first would refuse a call that works. `say` is a keyword and
    // no builtin at all, which is what makes it a good answer for the refusal.
    if let Some(shape) = stdlib::builtin_signature(name) {
        let mut lines = vec![format!("{} — builtin", name)];
        lines.extend(shape.lines().map(|line| format!("  {}", line)));
        return Some(lines);
    }

    if let Some(Value::Function(function)) = session_function {
        let params = if function.params.is_empty() {
            "(none)".to_string()
        } else {
            function.params.join(", ")
        };
        return Some(vec![
            format!("{} — session function", name),
            format!("  parameters: {}", params),
            format!("  arity:       {}", function.params.len()),
        ]);
    }

    // A keyword that reaches this point is not a callable at all: `say` answers
    // every time and takes no parentheses ever, which is a misunderstanding
    // worth refusing rather than reporting as an unknown name.
    if Lexer::keywords().contains(&name) {
        return None;
    }

    // A `Module.member` spelling is answered as the builtin behind it, so
    // `.sighelp files.read` and `.sighelp files_read` are one answer. A member
    // of a module this session built has no shape to read: its declaration is
    // `.which`'s to point at, not this table's to guess.
    match classify(name, None, false) {
        Origin::StdlibMember { module, member } => {
            sighelp_lines(&format!("{module}_{member}"), None)
        }
        _ => None,
    }
}

/// The `.type` answer: the value, what kind of thing it is, and where it came
/// from.
pub fn type_lines(value: &Value, definition: Option<&Definition>) -> Vec<String> {
    let mut lines = vec![
        format!("value: {}", value),
        format!("kind:  {} ({})", value.type_name(), shape_of(value)),
    ];
    match definition {
        Some(definition) => lines.push(format!("from:  {}  {}", definition, definition.source)),
        None => lines.push("from:  evaluated at the prompt".to_string()),
    }
    lines
}

/// How big a value is, in the terms of its own kind.
///
/// The kind alone says `[1, 2]` is a list; the shape says it holds two items,
/// which is the half a wrong guess about a value usually gets wrong.
fn shape_of(value: &Value) -> String {
    match value {
        Value::List(items) => format!("{} item(s)", items.len()),
        Value::Text(text) => format!("{} byte(s)", text.len()),
        Value::Record(fields) => format!("{} field(s)", fields.len()),
        Value::Nothing => "no value".to_string(),
        _ => "no size".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{classify, doc_lines, sighelp_lines, type_lines, Definition, Origin};
    use crate::stdlib;
    use crate::value::Value;

    fn at(line: usize) -> Definition {
        Definition {
            file: "<session>".to_string(),
            line,
            source: format!("set x to {}", line),
        }
    }

    /// The shape table and the registry are one table or they are two, and the
    /// direction that hurts is a builtin with no shape: `.sighelp` would go
    /// quiet about a function that exists.
    #[test]
    fn every_registered_builtin_has_a_written_shape() {
        assert_eq!(
            stdlib::builtins_without_signatures(),
            Vec::<&str>::new(),
            "every name in the registry answers :sighelp"
        );
        assert_eq!(
            stdlib::signatures_covering_nothing(),
            Vec::<&str>::new(),
            "no shape is written for a name the registry does not hold"
        );
    }

    /// `.doc` reads the module surface out of the registry, so a module cannot
    /// be listed here and be absent from the language, and a member added to
    /// [`stdlib::MODULE_MEMBERS`] cannot be missing from the answer.
    ///
    /// The expectation is derived rather than written out: a hand-written list
    /// of `json`'s two members would have kept passing after a third was added.
    #[test]
    fn doc_lists_exactly_what_the_registry_holds() {
        let lines = doc_lines("json").expect("json is a module");
        let listed: Vec<String> = lines
            .iter()
            .skip(1)
            .map(|line| {
                line.split_whitespace()
                    .next()
                    .unwrap_or_default()
                    .to_string()
            })
            .collect();
        let expected: Vec<String> = stdlib::MODULE_MEMBERS
            .iter()
            .find(|(module, _)| *module == "json")
            .map(|(_, members)| {
                members
                    .iter()
                    .map(|member| format!("json.{member}"))
                    .collect()
            })
            .expect("json is listed in the registry");

        assert_eq!(listed, expected, "every member of json is listed, once");

        // And nothing outside the registry is.
        for module in stdlib::MODULES {
            let lines = doc_lines(module).expect("a registered module has a doc listing");
            let members = lines.len() - 1;
            let expected = stdlib::MODULE_MEMBERS
                .iter()
                .find(|(listed, _)| listed == module)
                .map(|(_, members)| members.len())
                .unwrap_or(1);
            assert_eq!(
                members, expected,
                "the listing of {} is the registry's row for it",
                module
            );
        }
    }

    /// A module the language does not have is refused by name rather than
    /// answered with an empty listing, which reads like a module with no
    /// members and sends the user off to look for a bug in the REPL.
    #[test]
    fn edge_doc_refuses_a_module_that_is_not_one() {
        assert_eq!(
            doc_lines("nosuchmodule"),
            None,
            "a name that is not a module gets no listing"
        );
        assert_eq!(
            doc_lines(""),
            None,
            "and an empty name is not the module with no members"
        );
    }

    /// A session `set` shadows a builtin, and `.which` has to report the binding
    /// rather than the builtin — the binding is what the next line reaches.
    #[test]
    fn which_reports_the_session_binding_over_a_builtin() {
        let definition = at(4);
        let origin = classify("uppercase", Some(&definition), false);

        assert_eq!(
            origin,
            Origin::Session {
                name: "uppercase".to_string(),
                definition: definition.clone()
            },
            "a session binding shadows the builtin of the same name"
        );

        let lines = super::which_lines("uppercase", &origin);
        assert!(
            lines[0].contains("session binding"),
            "the answer names the session: {:?}",
            lines
        );
        assert!(
            lines.iter().any(|line| line.contains("<session>:4")),
            "and says where it was written: {:?}",
            lines
        );

        // Without the binding it is the builtin, which is what proves the order
        // above was a shadow and not the only answer.
        assert_eq!(
            classify("uppercase", None, false),
            Origin::Builtin {
                name: "uppercase".to_string()
            },
            "with nothing bound under it, the name is the builtin"
        );
    }

    /// A name nothing binds is unknown. Not a builtin, not an empty listing —
    /// the whole point of asking is to be told there is nothing.
    #[test]
    fn edge_which_says_unknown_for_a_name_nothing_binds() {
        for name in ["nosuchname", "", "up_percase", "uppercase "] {
            assert_eq!(
                classify(name, None, false),
                Origin::Unknown {
                    name: name.to_string()
                },
                "{:?} binds nothing",
                name
            );
        }
    }

    /// `:sighelp split` answers with the separator form, because the argument
    /// that is easy to leave out is the one people get wrong.
    #[test]
    fn sighelp_shows_the_separator_form_of_split() {
        let lines = sighelp_lines("split", None).expect("split is a builtin with a shape");
        let joined = lines.join("\n");

        assert!(
            joined.contains("split(text, separator)"),
            "the shape names both arguments: {:?}",
            lines
        );
        assert!(
            joined.contains("list of text"),
            "and what it answers: {:?}",
            lines
        );
    }

    /// A dotted spelling is answered as the builtin behind it, because
    /// `files.read` and `files_read` are one function and `:sighelp` is about
    /// the function.
    #[test]
    fn sighelp_answers_a_module_member_spelling() {
        let direct = sighelp_lines("files_read", None).expect("files_read has a shape");
        let dotted = sighelp_lines("files.read", None).expect("files.read resolves to it");

        assert_eq!(
            direct, dotted,
            "a dotted spelling and the name under it are one function"
        );
    }

    /// A keyword is not a callable. `:sighelp say` refuses, and refuses by
    /// saying what `say` is — a silent `None` would read as "no such function",
    /// and `say` answers every time.
    #[test]
    fn edge_sighelp_refuses_a_keyword_rather_than_inventing_a_shape() {
        assert_eq!(
            sighelp_lines("say", None),
            None,
            "a keyword is not something you call with parentheses"
        );
        assert_eq!(
            sighelp_lines("end", None),
            None,
            "and neither is the keyword that closes a block"
        );
    }

    /// `.type` answers three things, and the value is rendered by the language
    /// rather than by a Debug impl, so `[1, 2]` is what the user sees.
    #[test]
    fn type_reports_value_kind_and_size() {
        let lines = type_lines(
            &Value::list(vec![Value::Number(1.0), Value::Number(2.0)]),
            None,
        );
        let joined = lines.join("\n");

        assert!(joined.contains("value: [1, 2]"), "the value: {:?}", lines);
        assert!(
            joined.contains("kind:  list (2 item(s))"),
            "the kind and how much of it there is: {:?}",
            lines
        );
        assert!(
            joined.contains("evaluated at the prompt"),
            "an expression nobody defined says so rather than inventing a span: {:?}",
            lines
        );
    }

    /// A name the session bound carries the line it was bound on. This is the
    /// half `:inspect` never had: it printed the value and the kind and nothing
    /// about where either came from.
    #[test]
    fn type_points_at_the_line_a_name_was_bound_on() {
        let lines = type_lines(&Value::Number(3.0), Some(&at(2)));
        let joined = lines.join("\n");

        assert!(
            joined.contains("from:  <session>:2"),
            "the span names the file and the line: {:?}",
            lines
        );
        assert!(
            joined.contains("set x to 2"),
            "and quotes what is written there: {:?}",
            lines
        );
    }

    /// The empty shapes are the ones a caller reaches by accident, and each one
    /// has to render rather than panic or divide. The last case is the boundary
    /// the empty text sits on: a length in bytes is not a length in characters,
    /// and `é` is two bytes and an emoji is four.
    #[test]
    fn edge_type_describes_the_empty_values_and_counts_bytes() {
        let cases: Vec<(Value, &str)> = vec![
            (Value::Nothing, "no value"),
            (Value::list(Vec::new()), "0 item(s)"),
            (Value::Text(String::new()), "0 byte(s)"),
            (Value::Text("héllo".to_string()), "6 byte(s)"),
            (Value::Text("🙂".to_string()), "4 byte(s)"),
        ];

        for (value, shape) in cases {
            let lines = type_lines(&value, None);
            assert!(
                lines[1].contains(shape),
                "{:?} is described as {}: {:?}",
                value,
                shape,
                lines
            );
        }
    }
}
