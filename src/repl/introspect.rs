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
use crate::parser::{Expr, Program, Statement, Stmt};
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
            // The dotted spelling is what a program writes and what the user
            // typed, so it is what is printed. `stdlib::resolve` answers the
            // name the *registry* holds — `files_read`, or `uppercase` for
            // `text.uppercase` — which is an implementation detail of how a
            // module member is stored, and printing it beside "callable as
            // files.read" told the user two spellings of one function and gave
            // them no reason to prefer one.
            vec![
                head,
                format!("  of module {}", module),
                format!("  callable as {}.{}", module, member),
                format!("  registered  {}.{}", module, member),
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

/// The one expression `.type` will evaluate, or why it refuses the program.
///
/// `.type` is a question and a program is not a question. Run through the
/// session's own pipeline — which is what this command used to do — a program
/// binds names (`set x to 1`), prints (`say`), loads modules (`import`), and
/// leaves no trace of where: the line a name was bound on is read out of the
/// *source* by `record_definitions`, and a name bound by a command was written
/// on no line `:history` holds. So asking `:type set x to 1` left the session
/// holding an `x` that `.which x` would call unknown, and asking `:type say "hi"`
/// printed a line nobody asked for.
///
/// An expression is the whole of what the question needs: `nums`, `[1, 2]`,
/// `x + 1`, `files.read("a.rb")`. Anything else is named back to the user, with
/// the keyword they wrote, rather than being run and then reported about.
pub fn expression_of(program: &Program) -> Result<&Expr, String> {
    match program.statements.as_slice() {
        [Stmt {
            statement: Statement::Expr(expr),
            ..
        }] => Ok(expr),
        [Stmt { statement, .. }] => Err(format!(
            "Error: :type is a question about a value, and `{}` is a statement. \
             Type the expression you want described, or run the statement as a line.",
            statement_keyword(statement)
        )),
        [] => Err(
            "Error: :type was given nothing to describe. Type the expression you want described."
                .to_string(),
        ),
        statements => Err(format!(
            "Error: :type is a question about one value, and this is {} statement(s). \
             Type the expression you want described, or run the lines as a program.",
            statements.len()
        )),
    }
}

/// The keyword a statement is written with, for a refusal that names it.
///
/// `:type` never runs a statement, so the refusal is the only time the user
/// hears what they handed over; "not an expression" without the word they typed
/// is a worse answer than "that is a `set`".
fn statement_keyword(statement: &Statement) -> &'static str {
    match statement {
        Statement::Say(_) => "say",
        Statement::Expr(_) => "an expression",
        Statement::Print(_) => "print",
        Statement::Set { .. } => "set",
        Statement::Constant { .. } => "constant",
        Statement::SetProperty { .. } => "set",
        Statement::If { .. } => "if",
        Statement::Unless { .. } => "unless",
        Statement::ForEach { .. } | Statement::ForRange { .. } => "for each",
        Statement::Repeat { .. } | Statement::RepeatUntil { .. } => "repeat",
        Statement::While { .. } => "while",
        Statement::Break => "break",
        Statement::Skip => "skip",
        Statement::Return(_) => "return",
        Statement::GiveBack(_) => "give back",
        Statement::Function { .. } => "to",
        Statement::Method { .. } => "to",
        Statement::Has { .. } => "has",
        Statement::Object { .. } => "object",
        Statement::Try { .. } => "try",
        Statement::Test { .. } => "test",
        Statement::Import(_) => "import",
        Statement::Module { .. } => "module",
        Statement::Export { .. } => "export",
    }
}

/// One call an expression makes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    /// The receiver a dotted call is written on — `files` in `files.read(…)` —
    /// and `None` for a call written on nothing.
    pub receiver: Option<String>,
    /// The function's own name, however it was reached: `read` in both
    /// `files.read(…)` and `files_read(…)`.
    pub member: String,
}

impl Call {
    /// The name the registry holds this call under.
    ///
    /// `module` is the module the receiver *names*, which is the module rather
    /// than the alias: `import files as F` then `F.delete(…)` is `files_delete`,
    /// and a name the registry does not hold is what makes it answerable.
    pub fn builtin_name(&self, module: Option<&str>) -> String {
        match (module, &self.receiver) {
            (Some(module), Some(_)) => format!("{module}_{}", self.member),
            _ => self.member.clone(),
        }
    }
}

/// Every call `expression` makes, outermost first.
///
/// The whole tree, and not just its outermost expression: a call can be an
/// argument (`uppercase(files.read(p))`), a receiver (`files.lines(p)[0].trim()`),
/// an interpolated piece, or the body of a function literal passed to `map` —
/// `xs.map(to(x) files.write("f", x) end)` reaches the filesystem through an
/// expression that reads as a list operation. Reading only the top would make
/// every one of those an answer to `.type` with nothing said about what it does.
///
/// Statements inside a function literal are walked for the same reason: the
/// literal is a value until it is called, and the calls are what a caller has to
/// be told about before it calls it.
///
/// **Iterative, and that is load-bearing.** The walk runs on the REPL's own
/// thread — `.type` answers on the thread the user is typing on, not on the
/// sized one [`crate::interpreter::run_isolated`] spawns — so a recursive walk
/// would spend this process's stack on a tree built from one typed line and take
/// the whole REPL down when it ran out, instead of printing a diagnostic. The
/// parser's own depth limits make that unreachable today; a walk that *is* the
/// bound rather than a caller of it cannot become reachable by a later change
/// to either.
pub fn calls_in(expression: &Expr) -> Vec<Call> {
    let mut calls = Vec::new();
    let mut pending = vec![Pending::Expr(expression)];
    while let Some(item) = pending.pop() {
        match item {
            Pending::Expr(expr) => push_expression(expr, &mut calls, &mut pending),
            Pending::Body(body) => push_body(body, &mut calls, &mut pending),
        }
    }
    calls
}

/// Every call the statements of `body` make, outermost first.
///
/// The same walk as [`calls_in`], entered from a block instead of from one
/// expression, and for the same reasons. It is separate because the REPL has a
/// second entry into it: a question about `f()` has to read the body of the
/// `to` function `f` names, because that body is where the call that changes the
/// world actually is. A [`crate::value::FunctionBody`] holds its statements, and
/// walking them is the only way to know what a session-declared call does before
/// deciding whether to make it.
pub fn calls_in_body(body: &[Stmt]) -> Vec<Call> {
    let mut calls = Vec::new();
    let mut pending = vec![Pending::Body(body)];
    while let Some(item) = pending.pop() {
        match item {
            Pending::Expr(expr) => push_expression(expr, &mut calls, &mut pending),
            Pending::Body(body) => push_body(body, &mut calls, &mut pending),
        }
    }
    calls
}

/// A node of the walk still to be read.
///
/// Both halves of the tree in one stack, so a body inside an expression inside a
/// body is one list rather than a mutual recursion between two functions that
/// would have to know about each other.
enum Pending<'a> {
    Expr(&'a Expr),
    Body(&'a [Stmt]),
}

/// Records the calls `expression` makes itself and queues everything under it.
///
/// Children are queued **in reverse**, because the stack is a stack: the order
/// calls are recorded in is the order a reader of them expects, and that order is
/// the left-to-right order of the source.
fn push_expression<'a>(
    expression: &'a Expr,
    calls: &mut Vec<Call>,
    pending: &mut Vec<Pending<'a>>,
) {
    match expression {
        Expr::Number(_) | Expr::Text(_) | Expr::YesNo(_) | Expr::Nothing => {}
        Expr::Variable(..) => {}
        Expr::Binary { left, right, .. } => {
            pending.push(Pending::Expr(right));
            pending.push(Pending::Expr(left));
        }
        Expr::Unary { expr, .. } => pending.push(Pending::Expr(expr)),
        Expr::Call { name, args } => {
            calls.push(Call {
                receiver: None,
                member: name.clone(),
            });
            queue_args(args, pending);
        }
        Expr::Property { object, property } => {
            calls.push(Call {
                receiver: Some(receiver_name(object)),
                member: property.clone(),
            });
            pending.push(Pending::Expr(object));
        }
        Expr::MethodCall {
            receiver,
            method,
            args,
        } => {
            calls.push(Call {
                receiver: Some(receiver_name(receiver)),
                member: method.clone(),
            });
            queue_args(args, pending);
            pending.push(Pending::Expr(receiver));
        }
        Expr::Index { object, index } => {
            pending.push(Pending::Expr(index));
            pending.push(Pending::Expr(object));
        }
        Expr::InterpolatedText(parts) => {
            for part in parts.iter().rev() {
                pending.push(Pending::Expr(part));
            }
        }
        Expr::List(items) => {
            for item in items.iter().rev() {
                pending.push(Pending::Expr(item));
            }
        }
        Expr::Record(fields) => {
            for (_, value) in fields.iter().rev() {
                pending.push(Pending::Expr(value));
            }
        }
        Expr::FunctionLiteral { body, .. } => pending.push(Pending::Body(body)),
        Expr::Expect { actual, expected } => {
            pending.push(Pending::Expr(expected));
            pending.push(Pending::Expr(actual));
        }
        Expr::MightFail(inner) => pending.push(Pending::Expr(inner)),
    }
}

/// Queues a call's arguments so they are read left to right.
fn queue_args<'a>(args: &'a [Expr], pending: &mut Vec<Pending<'a>>) {
    for arg in args.iter().rev() {
        pending.push(Pending::Expr(arg));
    }
}

/// Records the calls the statements of `body` make and queues everything under
/// them, in source order.
///
/// Nothing is recorded here directly: a statement calls what its expressions
/// call, and [`push_expression`] is where an expression is read. The unused
/// parameter is kept so the two halves of the walk take the same shape, and a
/// statement form that does carry a call of its own has somewhere to put it.
fn push_body<'a>(body: &'a [Stmt], _calls: &mut Vec<Call>, pending: &mut Vec<Pending<'a>>) {
    for stmt in body.iter().rev() {
        match &stmt.statement {
            Statement::Say(expr) | Statement::Print(expr) | Statement::Expr(expr) => {
                pending.push(Pending::Expr(expr))
            }
            Statement::Set { value, .. }
            | Statement::Constant { value, .. }
            | Statement::SetProperty { value, .. }
            | Statement::Return(Some(value))
            | Statement::GiveBack(Some(value)) => pending.push(Pending::Expr(value)),
            Statement::ForEach { iterable, body, .. } => {
                pending.push(Pending::Body(body));
                pending.push(Pending::Expr(iterable));
            }
            Statement::ForRange {
                start,
                end,
                step,
                body,
                ..
            } => {
                pending.push(Pending::Body(body));
                if let Some(step) = step {
                    pending.push(Pending::Expr(step));
                }
                pending.push(Pending::Expr(end));
                pending.push(Pending::Expr(start));
            }
            Statement::Repeat { count, body } => {
                pending.push(Pending::Body(body));
                pending.push(Pending::Expr(count));
            }
            Statement::RepeatUntil {
                body, condition, ..
            } => {
                pending.push(Pending::Expr(condition));
                pending.push(Pending::Body(body));
            }
            Statement::While { condition, body } => {
                pending.push(Pending::Body(body));
                pending.push(Pending::Expr(condition));
            }
            Statement::If {
                condition,
                then_branch,
                else_branch,
            } => {
                pending.push(Pending::Body(else_branch));
                pending.push(Pending::Body(then_branch));
                pending.push(Pending::Expr(condition));
            }
            Statement::Unless { condition, body } => {
                pending.push(Pending::Body(body));
                pending.push(Pending::Expr(condition));
            }
            Statement::Function { body, .. }
            | Statement::Method { body, .. }
            | Statement::Test { body, .. }
            | Statement::Object { body, .. }
            | Statement::Module { body, .. } => pending.push(Pending::Body(body)),
            Statement::Try {
                body,
                catch_body,
                finally_body,
                ..
            } => {
                pending.push(Pending::Body(finally_body));
                pending.push(Pending::Body(catch_body));
                pending.push(Pending::Body(body));
            }
            Statement::Break
            | Statement::Skip
            | Statement::Return(None)
            | Statement::GiveBack(None)
            | Statement::Has { .. }
            | Statement::Import(_)
            | Statement::Export { .. } => {}
        }
    }
}

/// The receiver a dotted call is written on, as a name.
///
/// A receiver that is not a name — `lines[0].trim()`, `(a + b).length` — reads as
/// the empty string, which matches no module and therefore names no builtin: a
/// call on a value is not a call on the standard library, and refusing to guess
/// here is what stops a method on a record from being refused as though it were
/// one of the library's.
///
/// Iterative for the same reason [`calls_in`] is: the chain it walks is as deep
/// as the source the user typed, on the REPL's own stack.
fn receiver_name(expression: &Expr) -> String {
    let mut parts: Vec<&str> = Vec::new();
    let mut current = expression;
    loop {
        match current {
            Expr::Variable(name, _) => {
                parts.push(name.as_str());
                break;
            }
            Expr::Property { object, property } => {
                parts.push(property.as_str());
                current = object;
            }
            _ => return String::new(),
        }
    }
    parts.reverse();
    parts.join(".")
}

#[cfg(test)]
mod tests {
    use super::{
        calls_in, calls_in_body, classify, doc_lines, expression_of, sighelp_lines, type_lines,
        which_lines, Call, Definition, Origin,
    };
    use crate::parser::{Parser, Stmt};
    use crate::stdlib;
    use crate::value::Value;

    fn at(line: usize) -> Definition {
        Definition {
            file: "<session>".to_string(),
            line,
            source: format!("set x to {}", line),
        }
    }

    /// The program `code` parses to, for the tests below.
    fn program(code: &str) -> crate::parser::Program {
        let tokens = crate::lexer::Lexer::tokenize(code).expect("the source should lex");
        Parser::new(tokens)
            .parse()
            .expect("the source should parse")
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

    /// The third table is checked from both ends too. A refusal naming a builtin
    /// the language does not have refuses something that was never going to run,
    /// which reads as a bug in the command rather than as the rule.
    #[test]
    fn edge_every_effectful_name_is_a_registered_builtin() {
        assert_eq!(
            stdlib::effectful_names_that_are_not_builtins(),
            Vec::<&str>::new(),
            "every refusal names a builtin the registry holds"
        );

        // And the spellings agree: a dotted member, the name it is stored under,
        // and the bare member of an alias module are one answer.
        for name in ["files_delete", "files.delete", "network.get", "time.sleep"] {
            assert!(
                stdlib::is_effectful(name),
                "{} writes outside the value it answers",
                name
            );
        }
        for name in [
            "uppercase",
            "length",
            "text.uppercase",
            "list.length",
            "nope.delete",
        ] {
            assert!(
                !stdlib::is_effectful(name),
                "{} only answers a value, and must stay answerable",
                name
            );
        }
    }

    /// A read and a draw leave the process as surely as a write does, and the
    /// table treated them as though they did not — which is the whole defect
    /// this pins, from the table's side.
    ///
    /// The three ways round it are enumerated rather than described, because each
    /// one was available and each one is worse than refusing:
    ///
    /// - **snapshot and restore** the generator around the question: a question
    ///   asking `random()` twice gets the same answer twice and the *next* draw
    ///   a program makes is the one it would have made. That works, and it only
    ///   works for the state we know about — the clock and the filesystem are
    ///   not a `Cell<u64>` that can be put back.
    /// - **time out** the call: bounds the wait, but a `files.read` that completed
    ///   has already had its effect and a `time.now()` that returned has already
    ///   answered with something that cannot be un-answered.
    /// - **refuse**: the honest answer, and what `:sighelp` is for.
    ///
    /// Every name below is checked in both spellings, because the dotted one is
    /// what the user types and the flat one is what the gate compares.
    #[test]
    fn edge_a_read_a_draw_and_a_clock_reading_are_refused_as_effects_too() {
        for name in [
            "files_read",
            "files_exists",
            "files_lines",
            "time_now",
            "random",
            "random_number",
            "random_choice",
            "random_shuffle",
        ] {
            assert!(
                stdlib::is_effectful(name),
                "{} leaves the process even though it writes nothing: a read can \
                 block on a FIFO and forever, a draw moves the generator the next \
                 answer comes from, and a clock reading cannot be reproduced",
                name
            );

            let dotted = name.replacen('_', ".", 1);
            assert!(
                stdlib::is_effectful(&dotted),
                "{} and {} are one function, so they are one refusal",
                name,
                dotted
            );
        }

        // The other end, so the list cannot grow into "refuse everything": the
        // calls that really do only answer a value are still answerable. Each one
        // is a neighbour of a refused name, because that is where the mistake is
        // — `time_format` beside `time_now`, `random_seed` beside `random`.
        for name in [
            "time_format",
            "time_unix",
            "json.parse",
            "csv.parse",
            "sys.argv",
            "bytes.from_text",
            "list.length",
        ] {
            let dotted = name.replacen('_', ".", 1);
            assert!(
                !stdlib::is_effectful(&dotted),
                "{} answers from its arguments and changes nothing",
                dotted
            );
        }
    }

    /// A question is a question. Every statement is refused, by name, and one
    /// expression is the only thing accepted — the refusal names the keyword the
    /// user typed, because "not an expression" without it is a worse answer than
    /// "that is a `set`".
    #[test]
    fn edge_a_question_refuses_a_statement_and_names_the_keyword() {
        for (code, keyword) in [
            ("set x to 1", "set"),
            ("say \"hi\"", "say"),
            ("print \"hi\"", "print"),
            ("import files", "import"),
            ("to f()\n    return 1\nend", "to"),
            ("module M\n    export all\nend", "module"),
            ("if yes then say \"hi\" end", "if"),
            ("repeat 2 times\n    say \"hi\"\nend", "repeat"),
            ("test \"t\"\n    expect 1 to be 1\nend", "test"),
            ("constant C to 1", "constant"),
        ] {
            let refusal = expression_of(&program(code))
                .expect_err("a statement is not a question about a value");
            assert!(
                refusal.contains(keyword),
                "{:?} is refused naming {:?}, got: {}",
                code,
                keyword,
                refusal
            );
            assert!(
                refusal.starts_with("Error:"),
                "and is reported as the error it is, not as a value: {}",
                refusal
            );
        }

        // The boundaries: one expression is the whole of what is accepted, and
        // neither half of "one" is negotiable.
        assert!(expression_of(&program("[1, 2]")).is_ok(), "a literal");
        assert!(expression_of(&program("x + 1")).is_ok(), "an operation");
        assert!(
            expression_of(&program("files.read(\"a.rb\")")).is_ok(),
            "a call is still an expression"
        );
        assert!(
            expression_of(&program("x = 1\ny = 2")).is_err(),
            "two statements are two values, and :type answers about one"
        );
        assert!(
            expression_of(&program("")).is_err(),
            "an empty program is not an expression, it is nothing to describe"
        );
    }

    /// Every call an expression makes is found, wherever it is written — because
    /// a refusal that only reads the outermost expression is a refusal that can
    /// be written around.
    #[test]
    fn edge_calls_are_found_wherever_an_expression_hides_one() {
        let found = |code: &'static str| {
            expression_of(&program(code))
                .map(calls_in)
                .expect("these are all expressions")
        };

        assert_eq!(
            found("files.read(p)"),
            vec![Call {
                receiver: Some("files".to_string()),
                member: "read".to_string()
            }],
            "a dotted call is a call"
        );
        assert_eq!(
            found("uppercase(files.delete(p))"),
            vec![
                Call {
                    receiver: None,
                    member: "uppercase".to_string()
                },
                Call {
                    receiver: Some("files".to_string()),
                    member: "delete".to_string()
                },
            ],
            "a call nested in an argument is still a call"
        );
        assert_eq!(
            found("files.lines(p)[0].trim()")
                .iter()
                .map(|call| call.member.clone())
                .collect::<Vec<String>>(),
            vec!["trim", "lines"],
            "a call on the result of a call is found, and so is the one under it"
        );
        assert_eq!(
            found("xs.map(to(x) files.write(\"f\", x) end)")
                .iter()
                .map(|call| call.member.clone())
                .collect::<Vec<String>>(),
            vec!["map", "write"],
            "a call inside a function literal is found: the literal is inert until \
             something calls it, and `map` calls it"
        );

        // And the registry name is the one the registry holds, whichever way it
        // was written and whichever name the module was imported under.
        let dotted = found("files.delete(p)")[0].builtin_name(Some("files"));
        assert_eq!(dotted, "files_delete");
        let aliased = found("F.write(p, \"x\")")[0].builtin_name(Some("files"));
        assert_eq!(
            aliased, "files_write",
            "an alias is resolved to its module before the name is built"
        );
        assert_eq!(
            found("files_read(p)")[0].builtin_name(None),
            "files_read",
            "a flat call is already the registry's name"
        );
    }

    /// A method call on a *value* is not a call on the standard library, and
    /// guessing that it is would refuse `"a,b".split(",")[0].length` as though
    /// the language had no way to write it.
    #[test]
    fn edge_a_method_call_on_a_value_is_not_named_after_a_module() {
        let found = |code: &'static str| {
            expression_of(&program(code))
                .map(calls_in)
                .expect("these are all expressions")
        };

        for code in [
            "\"a,b\".split(\",\")[0].length()",
            "rec.trim()",
            "items.length",
        ] {
            for call in found(code) {
                assert!(
                    !stdlib::is_effectful(&call.builtin_name(None)),
                    "{} must not resolve to a name the effect table refuses",
                    code
                );
                assert!(
                    !call.builtin_name(None).starts_with('_'),
                    "a receiver that is not a module names no module: {:?}",
                    call
                );
            }
        }
    }

    /// A statement block is a second door into the same walk, and the door the
    /// REPL needs is the one a `to` function's body comes through: a question
    /// about `f()` has to be able to read what `f` does, and that is a body and
    /// not an expression.
    ///
    /// Both ends are checked because the walk is iterative now and an explicit
    /// stack is exactly the kind of thing that gets the *order* wrong: children
    /// are queued in reverse so they are popped in source order, and a list
    /// built backwards reads as a correct list of the wrong calls.
    #[test]
    fn edge_a_body_is_walked_in_source_order_and_stops_at_its_end() {
        let members = |body: &[Stmt]| -> Vec<String> {
            calls_in_body(body)
                .into_iter()
                .map(|call| call.member)
                .collect()
        };
        let statements = |code: &str| -> Vec<Stmt> { program(code).statements };

        assert_eq!(
            members(&statements("set a to f()\nsay g()\nreturn h()")),
            vec!["f", "g", "h"],
            "statements are read in the order they are written, not stacked"
        );

        // Every statement form that carries an expression, including the ones
        // only reachable inside a branch — a refusal that read the straight line
        // would miss the `else` entirely.
        assert_eq!(
            members(&statements(
                "if a() then b() else c() end\nunless d() then e() end"
            )),
            vec!["a", "b", "c", "d", "e"],
            "both branches of a conditional are walked"
        );
        assert_eq!(
            members(&statements(
                "for each x in xs()\n    say y()\nend\nrepeat n() times\n    say z()\nend"
            )),
            vec!["xs", "y", "n", "z"],
            "a loop's iterable and its body are both walked"
        );
        assert_eq!(
            members(&statements("to inner()\n    files.delete(p)\nend")),
            vec!["delete"],
            "a nested declaration is walked too: this is the shape the callee walk reads"
        );
        assert_eq!(
            members(&statements("set x to [a(), b()]")),
            vec!["a", "b"],
            "and the left-to-right order survives a list literal"
        );

        // The boundaries: nothing in, nothing out.
        assert_eq!(members(&[]), Vec::<String>::new(), "an empty body");
        assert_eq!(
            members(&statements("break")),
            Vec::<String>::new(),
            "a statement with no expression makes no call"
        );
        assert_eq!(
            members(&statements("files.lines(p)[0].trim()")),
            vec!["trim", "lines"],
            "a dotted receiver is read inside-out, as the expression walk reads it"
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

    /// A module member is printed the way it is *called*, and the registry's own
    /// encoding is not printed at all.
    ///
    /// `.which files.read` used to end with `builtin    files_read` — the name
    /// the VM happens to store the function under, printed beside
    /// `callable as files.read`. Two spellings of one function, one of them not
    /// a thing a program can write, and no reason given to prefer either. It is
    /// worse than untidy for the `ALIAS_MODULES`: `text.uppercase` resolves to
    /// the bare `uppercase`, so that line read
    ///
    /// ```text
    ///   callable as text.uppercase
    ///   builtin    uppercase
    /// ```
    ///
    /// which is not an implementation detail leaking out, it is the answer to a
    /// question the user did not ask, filed under a question they did.
    #[test]
    fn edge_which_prints_a_member_the_way_it_is_called_and_never_the_registrys_spelling() {
        for (name, module, member) in [
            ("files.read", "files", "read"),
            ("json.parse", "json", "parse"),
            ("time.now", "time", "now"),
            ("text.uppercase", "text", "uppercase"),
        ] {
            let origin = classify(name, None, false);
            assert_eq!(
                origin,
                Origin::StdlibMember {
                    module: module.to_string(),
                    member: member.to_string()
                },
                "{} is a module member in the registry",
                name
            );

            // The whole listing, exactly. A `contains` would not catch the leak
            // for `text.uppercase`, whose registry name `uppercase` is a substring
            // of the dotted spelling that must be there — and an exact listing is
            // the stronger claim anyway: it pins that nothing *else* crept in.
            assert_eq!(
                which_lines(name, &origin),
                vec![
                    format!("{name}: module member"),
                    format!("  of module {module}"),
                    format!("  callable as {module}.{member}"),
                    format!("  registered  {module}.{member}"),
                ],
                "{name} is printed entirely in the spelling a program writes"
            );

            let qualified = format!("{module}_{member}");
            let internal = stdlib::resolve(&qualified);
            assert_ne!(
                internal, name,
                "{name} is expected to differ from its registry spelling here; if it does \
                 not, this case no longer tests anything about the leak"
            );
        }

        // A session member is the same shape of answer, so it gets the same
        // spelling — there is nothing about it that would justify leaking one.
        assert_eq!(
            which_lines(
                "MathUtils.circle_area",
                &Origin::SessionMember {
                    module: "MathUtils".to_string(),
                    member: "circle_area".to_string()
                },
            ),
            vec![
                "MathUtils.circle_area: session module member".to_string(),
                "  of module MathUtils".to_string(),
                "  callable as MathUtils.circle_area".to_string(),
                "  registered  MathUtils.circle_area".to_string(),
            ],
            "a session module member is printed the same way"
        );
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
