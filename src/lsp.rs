//! Editor and language-server scaffolding.
//!
//! Two things live here, and both exist so tooling can attach to Redblue
//! without a build step or a Node toolchain:
//!
//! * [`textmate_grammar`] renders the TextMate grammar for `.rb` files. It is
//!   generated from [`crate::lexer::KEYWORDS`] — the same table the lexer
//!   matches through — so the highlighting cannot drift from the language. The
//!   rendered file ships at [`GRAMMAR_PATH`] and `cargo test` fails if the
//!   checked-in copy is stale.
//! * [`diagnostics`] turns a source file into the spanned lexer, parser and
//!   analyzer errors (plus linter findings) that an editor shows in its
//!   Problems panel.
//!
//! Positions are 1-based lines and 1-based **character** columns, matching
//! [`crate::Span`]: an editor drawing a caret under the reported column lines up
//! even when the line holds multi-byte characters.

use std::collections::{HashMap, HashSet};
use std::ops::Range;

use crate::error::Error;
use crate::lexer::{Lexer, Token, TokenKind, KEYWORDS};
use crate::parser::is_complete_expression;
use crate::stdlib::{is_module, BUILTIN_NAMES, MODULES, MODULE_MEMBERS};

/// Where the generated grammar is checked in. Nothing has to be built to make
/// the grammar valid: the file is plain JSON, and this constant names it.
pub const GRAMMAR_PATH: &str = "tooling/vscode/syntaxes/redblue.tmLanguage.json";

/// Where the editor's language configuration is checked in — the manifest that
/// tells the editor which marker `Ctrl+/` inserts and which brackets to pair.
/// It is hand-written JSON rather than generated, so [`cargo test`](crate)
/// pins its comment marker against the lexer the same way it pins the grammar.
pub const LANGUAGE_CONFIG_PATH: &str = "tooling/vscode/language-configuration.json";

/// How serious a diagnostic is. The names are the ones the LSP calls
/// `DiagnosticSeverity`, so a future language server can map them unchanged;
/// the number LSP actually puts on the wire is not what [`Diagnostic::to_json`]
/// emits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

impl Severity {
    /// The severity's name, spelled the way LSP spells `DiagnosticSeverity`.
    pub fn as_str(&self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
    }
}

/// One thing wrong with a source file, located in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub line: usize,
    pub column: usize,
    pub message: String,
    pub severity: Severity,
}

impl Diagnostic {
    fn new(line: usize, column: usize, message: String, severity: Severity) -> Self {
        Self {
            line,
            column,
            message,
            severity,
        }
    }

    /// A diagnostic for a compile-stage [`Error`], positioned by its [`crate::Span`].
    ///
    /// An error with no span (an `Io` failure, or one reported as
    /// [`crate::Span::unknown`]) has nowhere to point, so it is placed at the
    /// first character of the file rather than dropped: the editor still has to
    /// show the message.
    fn from_error(error: &Error) -> Self {
        let (line, column) = match error.span() {
            Some(span) => (span.line, span.column),
            None => (1, 1),
        };
        let message = error.message().to_string();

        Self::new(line, column, message, Severity::Error)
    }

    /// This diagnostic as a JSON object: `{line, column, severity, message}`.
    ///
    /// This is the shape `rb diagnostics` prints and the editor tooling
    /// consumes. It is deliberately *not* the LSP `Diagnostic` object: LSP
    /// carries a `range` of 0-based `{line, character}` positions and a numeric
    /// severity, while this carries 1-based [`crate::Span`] positions and a
    /// severity name, which is what an editor-facing line needs. A language
    /// server translates; it does not forward these bytes.
    pub fn to_json(&self) -> String {
        format!(
            concat!(
                "{{\"line\":{},\"column\":{},\"severity\":\"{}\",",
                "\"message\":{}}}"
            ),
            self.line,
            self.column,
            self.severity.as_str(),
            escape_json(&self.message),
        )
    }
}

/// Every problem the compiler finds in `source`, in the order an editor should
/// show them.
///
/// The source is lexed and parsed exactly once: the tree the linter reads is the
/// one just parsed ([`crate::linter::lint_program`]), so `rb diagnostics` does not
/// run the frontend twice per file. The findings are in a fixed order for a
/// given source — the linter sorts its unused-variable warnings by name — so the
/// JSON an editor diffs between saves does not churn.
///
/// The compile stages stop at the first failure — there is no program to analyze
/// once lexing or parsing fails — so at most one error comes from the frontend
/// itself. Once the program parses, everything else comes from
/// [`crate::linter::lint_program`], **which asks the analyzer too**: an editor and
/// `rb lint` must not give two answers about the same file.
///
/// This used to run the analyzer here and `return` on its refusal, which dropped
/// the linter's own findings for every file the analyzer refuses — the one case
/// where a reader editing a half-written file most wants to see both the blocking
/// error and what else is wrong. The analyzer's verdict now arrives through the
/// same shared step `rb lint` uses, and it arrives first among the errors,
/// because the linter's own tree walk produces warnings only.
pub fn diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut found = Vec::new();

    let tokens = match crate::lexer::Lexer::tokenize(source) {
        Ok(tokens) => tokens,
        Err(error) => {
            found.push(Diagnostic::from_error(&error));
            return found;
        }
    };

    let ast = match crate::parser::parse(tokens) {
        Ok(ast) => ast,
        Err(error) => {
            found.push(Diagnostic::from_error(&error));
            return found;
        }
    };

    let (errors, warnings) = crate::linter::lint_program(&ast);
    for error in errors {
        let (line, column) = known_position(error.line, error.column);
        found.push(Diagnostic::new(
            line,
            column,
            error.message,
            Severity::Error,
        ));
    }
    for warning in warnings {
        let (line, column) = known_position(warning.line, warning.column);
        found.push(Diagnostic::new(
            line,
            column,
            warning.message,
            Severity::Warning,
        ));
    }

    found
}

/// A source position, or the first character of the file when the producer has
/// no real one. The linter reports `0:0` for a whole-program finding, which no
/// editor can place, so it is folded the same way a span-less error is.
fn known_position(line: usize, column: usize) -> (usize, usize) {
    if line > 0 {
        (line, column.max(1))
    } else {
        (1, 1)
    }
}

/// [`diagnostics`] as the JSON array an editor or language server consumes.
pub fn diagnostics_json(source: &str) -> String {
    diagnostics_to_json(&diagnostics(source))
}

/// Already-computed [`Diagnostic`]s as the same JSON array [`diagnostics_json`]
/// produces, so a caller that has the findings does not have to run the
/// frontend a second time to render them.
pub fn diagnostics_to_json(found: &[Diagnostic]) -> String {
    let entries: Vec<String> = found.iter().map(Diagnostic::to_json).collect();

    format!("[{}]", entries.join(","))
}

/// A span a navigation answer points at. Lines and columns are 1-based
/// characters, the same positions [`Diagnostic`] reports, so an editor can draw
/// the same caret under either.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Location {
    pub line: usize,
    pub column: usize,
    pub end_line: usize,
    pub end_column: usize,
}

impl Location {
    fn of_name(token: &Token, name: &str) -> Self {
        let width = name.chars().count().max(1);
        Self {
            line: token.line,
            column: token.column,
            end_line: token.line,
            end_column: token.column + width - 1,
        }
    }

    fn to_json(self) -> String {
        format!(
            "{{\"line\":{},\"column\":{},\"end_line\":{},\"end_column\":{}}}",
            self.line, self.column, self.end_line, self.end_column,
        )
    }
}

/// What a goto-definition answer resolved to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefinitionKind {
    Variable,
    Constant,
    Parameter,
    Function,
    Import,
    LoopVariable,
    Object,
    Module,
    Catch,
    Builtin,
}

impl DefinitionKind {
    /// The kind's name, as the JSON answer spells it.
    pub fn as_str(&self) -> &'static str {
        match self {
            DefinitionKind::Variable => "variable",
            DefinitionKind::Constant => "constant",
            DefinitionKind::Parameter => "parameter",
            DefinitionKind::Function => "function",
            DefinitionKind::Import => "import",
            DefinitionKind::LoopVariable => "loop",
            DefinitionKind::Object => "object",
            DefinitionKind::Module => "module",
            DefinitionKind::Catch => "catch",
            DefinitionKind::Builtin => "builtin",
        }
    }
}

/// A name's declaration, as goto-definition answers it.
///
/// A builtin resolves through the registry, not a file, so its `line` and
/// `column` are `0` and [`DefinitionKind::Builtin`] says why. A file binding's
/// span covers the declared name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Definition {
    pub name: String,
    pub kind: DefinitionKind,
    pub line: usize,
    pub column: usize,
    pub end_line: usize,
    pub end_column: usize,
}

impl Definition {
    fn from_binding(binding: &Binding) -> Self {
        Self {
            name: binding.name.clone(),
            kind: binding.kind,
            line: binding.location.line,
            column: binding.location.column,
            end_line: binding.location.end_line,
            end_column: binding.location.end_column,
        }
    }

    fn builtin(name: &str) -> Self {
        Self {
            name: name.to_string(),
            kind: DefinitionKind::Builtin,
            line: 0,
            column: 0,
            end_line: 0,
            end_column: 0,
        }
    }

    fn to_json(&self) -> String {
        format!(
            concat!(
                "{{\"line\":{},\"column\":{},\"end_line\":{},\"end_column\":{},",
                "\"kind\":{},\"name\":{}}}"
            ),
            self.line,
            self.column,
            self.end_line,
            self.end_column,
            escape_json(self.kind.as_str()),
            escape_json(&self.name),
        )
    }
}

/// The source positions of one file, in characters, for translating editor
/// positions to token offsets and back.
struct SourceMap {
    chars: Vec<char>,
    line_starts: Vec<usize>,
}

impl SourceMap {
    fn new(source: &str) -> Self {
        let chars: Vec<char> = source.chars().collect();
        let mut line_starts = vec![0];
        let mut index = 0;
        while index < chars.len() {
            match chars[index] {
                '\n' => {
                    line_starts.push(index + 1);
                    index += 1;
                }
                '\r' => {
                    // A CRLF pair is one line break; a lone CR is the classic-Mac
                    // one. Both put the next line after the break, not at the CR.
                    let width = if chars.get(index + 1) == Some(&'\n') {
                        2
                    } else {
                        1
                    };
                    line_starts.push(index + width);
                    index += width;
                }
                _ => index += 1,
            }
        }
        Self { chars, line_starts }
    }

    fn offset(&self, line: usize, column: usize) -> usize {
        let start = self
            .line_starts
            .get(line.saturating_sub(1))
            .copied()
            .unwrap_or(self.chars.len());
        start.saturating_add(column.saturating_sub(1))
    }

    /// The source offset of a 1-based line and column, refusing positions no
    /// editor could have sent: line or column zero, a line past the file, or a
    /// column past the end of its line.
    fn validate(&self, line: usize, column: usize) -> Result<usize, String> {
        if line == 0 {
            return Err("line 0 is not a source position: lines are 1-based".to_string());
        }
        if column == 0 {
            return Err("column 0 is not a source position: columns are 1-based".to_string());
        }
        if line > self.line_starts.len() {
            return Err(format!("line {line} is past the end of the file"));
        }

        let start = self.line_starts[line - 1];
        let mut end = start;
        while end < self.chars.len() && self.chars[end] != '\n' && self.chars[end] != '\r' {
            end += 1;
        }
        if column > end - start + 1 {
            return Err(format!("column {column} is past the end of line {line}"));
        }

        Ok(start + column - 1)
    }
}

#[derive(Debug, Clone)]
struct Binding {
    name: String,
    location: Location,
    kind: DefinitionKind,
    /// Whether the name participates in lexical lookup. An object field is a
    /// member reached through `this` or the object's name, not a variable the
    /// analyzer binds in the object body, so it keeps a declaration and a
    /// member table entry without entering a scope.
    lexical: bool,
    /// Whether the name answers only a call, never a bare read. An object's
    /// method is reached as `name()` or through a receiver; the analyzer does
    /// not bind it as a lexical variable, so `give back name` inside the method
    /// body names an unknown variable rather than the method itself.
    call_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FrameKind {
    If,
    Unless,
    For,
    While,
    Repeat,
    To,
    Literal,
    Object,
    Module,
    Test,
    Try,
}

#[derive(Debug, Clone, Copy)]
struct Frame {
    kind: FrameKind,
    opens_scope: bool,
    catch_scope: bool,
    /// The object binding whose body this frame encloses, when a `has`
    /// declaration's fields are in scope inside it.
    object: Option<usize>,
    /// The module binding whose body this frame encloses, when a `module …
    /// end` declaration's top-level names are being collected as its members.
    module: Option<usize>,
    /// For a one-line function literal, the token index where the parsed body
    /// ends. The scope closes just before that token, so a trailing argument on
    /// the same line resolves outside the literal.
    closes_at_token: Option<usize>,
}

impl Frame {
    fn block(kind: FrameKind) -> Self {
        Self {
            kind,
            opens_scope: false,
            catch_scope: false,
            object: None,
            module: None,
            closes_at_token: None,
        }
    }

    fn scoped(kind: FrameKind) -> Self {
        Self {
            kind,
            opens_scope: true,
            catch_scope: false,
            object: None,
            module: None,
            closes_at_token: None,
        }
    }
}

struct Analysis {
    tokens: Vec<Token>,
    bindings: Vec<Binding>,
    /// For every token, the binding it declares or resolves to. `None` is an
    /// unresolved name (a builtin, an import member, or an unknown word).
    token_binding: Vec<Option<usize>>,
    /// Identifiers that name an object field through a `.`, not a lexical
    /// binding.
    member_names: HashSet<usize>,
    /// Identifiers inside a type annotation, which name a type rather than the
    /// variable of the same spelling.
    type_names: HashSet<usize>,
    /// Identifiers that are contextual syntax rather than a name: the handler
    /// word, `of` and the failure kind of a typed catch, and `default` in a
    /// field declaration. A variable of the same spelling must not answer for
    /// them.
    syntax_names: HashSet<usize>,
    /// The names visible where analysis stopped, for completion.
    visible: Vec<String>,
    /// An object's fields and methods, keyed by the object's binding. Kept for
    /// completion inside a `receiver.member` slot; goto-definition uses the
    /// same tables while it walks.
    object_fields: HashMap<usize, HashMap<String, usize>>,
    object_methods: HashMap<usize, HashMap<String, usize>>,
    /// A child object binding to the parent it extends.
    object_parents: HashMap<usize, usize>,
    /// A `this` token to the object whose body encloses it.
    this_objects: HashMap<usize, usize>,
    /// An import name or alias to the module it names, mirroring the runtime's
    /// `module_aliases`. A standard module is in the table under its own name
    /// even when it is imported unaliased.
    module_aliases: HashMap<String, String>,
    /// The module binding an import named at its own site, keyed by the import
    /// binding. A later same-named module cannot change what the import means.
    import_module_bindings: HashMap<usize, usize>,
    /// The names a program's own `module … end` declaration publishes, keyed by
    /// the module's name. A declared module is a namespace the program built,
    /// so its members are read from its `export` rather than from the stdlib's
    /// [`MODULE_MEMBERS`] table.
    declared_module_members: HashMap<String, Vec<String>>,
    /// The same published names keyed by the module binding, so an import alias
    /// keeps the members of the module it actually named.
    declared_module_members_by_binding: HashMap<usize, Vec<String>>,
}

fn visible_names(scopes: &[HashMap<String, usize>]) -> Vec<String> {
    scopes
        .iter()
        .rev()
        .flat_map(|scope| scope.keys().cloned())
        .collect()
}

fn declare(
    scopes: &mut [HashMap<String, usize>],
    bindings: &mut Vec<Binding>,
    binding_scopes: &mut Vec<usize>,
    scope_id: usize,
    name: &str,
    location: Location,
    kind: DefinitionKind,
) -> usize {
    let scope = scopes.last_mut().expect("the root scope always exists");
    if let Some(existing) = scope.get(name) {
        return *existing;
    }

    let id = bindings.len();
    bindings.push(Binding {
        name: name.to_string(),
        location,
        kind,
        lexical: true,
        call_only: false,
    });
    binding_scopes.push(scope_id);
    scope.insert(name.to_string(), id);
    id
}

/// Declare a module name, giving a redeclaration its own binding.
///
/// The runtime refuses a second `module` of the same name, but navigation still
/// sees both bodies. A fresh binding keeps each body's published table from
/// answering for the other's uses, while the scope points at the latest
/// declaration for the uses that follow it.
fn declare_module_binding(
    scopes: &mut [HashMap<String, usize>],
    bindings: &mut Vec<Binding>,
    binding_scopes: &mut Vec<usize>,
    scope_id: usize,
    name: &str,
    location: Location,
) -> usize {
    let id = bindings.len();
    bindings.push(Binding {
        name: name.to_string(),
        location,
        kind: DefinitionKind::Module,
        lexical: true,
        call_only: false,
    });
    binding_scopes.push(scope_id);
    let scope = scopes.last_mut().expect("the root scope always exists");
    scope.insert(name.to_string(), id);
    id
}

/// Declare a name that must stay distinct from a same-named lexical binding of
/// another kind in the same scope — an object's method and a plain name can
/// share a spelling but not an entry. The scope keeps the first binding of the
/// name for plain uses.
fn declare_fresh(
    scopes: &mut [HashMap<String, usize>],
    bindings: &mut Vec<Binding>,
    binding_scopes: &mut Vec<usize>,
    scope_id: usize,
    name: &str,
    location: Location,
    kind: DefinitionKind,
) -> usize {
    let id = bindings.len();
    bindings.push(Binding {
        name: name.to_string(),
        location,
        kind,
        lexical: true,
        call_only: kind == DefinitionKind::Function,
    });
    binding_scopes.push(scope_id);
    let scope = scopes.last_mut().expect("the root scope always exists");
    scope.entry(name.to_string()).or_insert(id);
    id
}

/// Declare an object field as a member rather than a lexical name.
///
/// The analyzer does not bind a `has` field as a variable in the object's body:
/// a bare `name` there is an unknown variable, and the field is read through
/// `this.name` or `<object>.name`. The field still needs a binding for its own
/// declaration span and for member access, so it is recorded in the member
/// table without entering a scope.
fn declare_member(
    bindings: &mut Vec<Binding>,
    binding_scopes: &mut Vec<usize>,
    scope_id: usize,
    name: &str,
    location: Location,
) -> usize {
    let id = bindings.len();
    bindings.push(Binding {
        name: name.to_string(),
        location,
        kind: DefinitionKind::Variable,
        lexical: false,
        call_only: false,
    });
    binding_scopes.push(scope_id);
    id
}

/// Declare a `constant` in the program scope.
///
/// A constant binds the name in the same global slot a `set` writes, so it
/// shadows an earlier `set` of that name rather than joining its binding: the
/// declaration token and every later use name the constant, while the earlier
/// variable keeps its own declaration and uses. A second `constant` of the same
/// name reuses the one binding, because the runtime refuses the redeclaration
/// and there is only ever one constant of a spelling.
fn declare_constant_binding(
    scopes: &mut [HashMap<String, usize>],
    bindings: &mut Vec<Binding>,
    binding_scopes: &mut Vec<usize>,
    scope_id: usize,
    name: &str,
    location: Location,
) -> usize {
    if let Some(existing) = scopes.last().and_then(|scope| scope.get(name).copied()) {
        if bindings[existing].kind == DefinitionKind::Constant {
            return existing;
        }
    }

    let id = bindings.len();
    bindings.push(Binding {
        name: name.to_string(),
        location,
        kind: DefinitionKind::Constant,
        lexical: true,
        call_only: false,
    });
    binding_scopes.push(scope_id);
    let scope = scopes.last_mut().expect("the root scope always exists");
    scope.insert(name.to_string(), id);
    id
}

/// Declare a function in the current scope.
///
/// A function and a variable of the same spelling share one slot in the
/// runtime: a later `to` replaces an earlier `set`, and every later use names
/// the function. A repeated `to` reuses the function binding, because there is
/// only ever one function of a spelling in a scope. A method keeps a distinct
/// binding through [`declare_fresh`] so it can coexist with a same-named field.
fn declare_function_binding(
    scopes: &mut [HashMap<String, usize>],
    bindings: &mut Vec<Binding>,
    binding_scopes: &mut Vec<usize>,
    scope_id: usize,
    name: &str,
    location: Location,
) -> usize {
    let scope = scopes.last_mut().expect("the root scope always exists");
    if let Some(existing) = scope.get(name).copied() {
        if bindings[existing].kind == DefinitionKind::Function {
            return existing;
        }
    }

    let id = bindings.len();
    bindings.push(Binding {
        name: name.to_string(),
        location,
        kind: DefinitionKind::Function,
        lexical: true,
        call_only: false,
    });
    binding_scopes.push(scope_id);
    scope.insert(name.to_string(), id);
    id
}

/// Declare a variable that replaces another same-named declaration in the
/// runtime scope at `scope_position`.
///
/// A `set` writes through to a variable-like slot but replaces a declaration of
/// another kind: a function, constant, import, object or module is a different
/// binding, and the later `set` owns the following uses while the earlier
/// declaration keeps its own. `scope_position` is the innermost function,
/// module, test or program frame: block scopes are analyzer constructs, not
/// places where the VM's `set` creates a variable.
fn declare_replacing_binding(
    scopes: &mut [HashMap<String, usize>],
    bindings: &mut Vec<Binding>,
    binding_scopes: &mut Vec<usize>,
    scope_id: usize,
    scope_position: usize,
    name: &str,
    location: Location,
) -> usize {
    let id = bindings.len();
    bindings.push(Binding {
        name: name.to_string(),
        location,
        kind: DefinitionKind::Variable,
        lexical: true,
        call_only: false,
    });
    binding_scopes.push(scope_id);
    scopes[scope_position].insert(name.to_string(), id);
    // When the runtime frame is an outer function, module, test or program and
    // the `set` appears in a block scope, the block may still hold the
    // declaration the `set` just replaced. Lookup walks the innermost scope
    // first, so leaving a stale function there would answer the shadowed
    // declaration. Point the current block at the new variable too; the block
    // is popped at its `end`, while the binding itself lives in the runtime
    // frame.
    if scope_position + 1 != scopes.len() {
        scopes
            .last_mut()
            .expect("the root scope always exists")
            .insert(name.to_string(), id);
    }
    id
}

/// Whether a `set` reuses an existing binding rather than replacing it.
///
/// A variable, parameter, loop variable and catch binder hold an assignable
/// runtime value, so a later `set` writes through to that slot. A function,
/// constant, import, object or module is a different declaration, and the
/// latest `set` starts a new variable binding instead.
fn set_reuses_binding(kind: DefinitionKind) -> bool {
    matches!(
        kind,
        DefinitionKind::Variable
            | DefinitionKind::Parameter
            | DefinitionKind::LoopVariable
            | DefinitionKind::Catch
    )
}

fn mark_declaration(
    token_binding: &mut [Option<usize>],
    declarations: &mut HashSet<usize>,
    index: usize,
    binding: usize,
) {
    declarations.insert(index);
    if let Some(slot) = token_binding.get_mut(index) {
        *slot = Some(binding);
    }
}

fn name_of(token: &Token) -> Option<&str> {
    match &token.kind {
        TokenKind::Identifier(name) => Some(name.as_str()),
        _ => None,
    }
}

fn skip_type_expression(
    tokens: &[Token],
    start: usize,
    stop_at_to: bool,
    type_names: &mut HashSet<usize>,
) -> usize {
    let mut cursor = start;
    let mut depth = 0usize;

    while cursor < tokens.len() {
        if matches!(tokens[cursor].kind, TokenKind::Identifier(_)) {
            type_names.insert(cursor);
        }
        match &tokens[cursor].kind {
            TokenKind::LeftParen | TokenKind::LeftBracket | TokenKind::LeftBrace => depth += 1,
            TokenKind::RightParen | TokenKind::RightBracket | TokenKind::RightBrace => {
                if depth == 0 {
                    return cursor;
                }
                depth -= 1;
            }
            TokenKind::Comma | TokenKind::Newline | TokenKind::Eof if depth == 0 => {
                return cursor;
            }
            TokenKind::To if depth == 0 && stop_at_to => return cursor,
            _ => {}
        }
        cursor += 1;
    }

    cursor
}

fn catch_binder_index(tokens: &[Token], catch_index: usize) -> Option<usize> {
    let first_index = catch_index + 1;
    let first = tokens.get(first_index)?;
    let first_name = name_of(first)?;

    if first_name != "error" {
        return Some(first_index);
    }

    let Some(second) = tokens.get(first_index + 1) else {
        return Some(first_index);
    };
    if second.line != first.line || name_of(second).is_none() {
        return Some(first_index);
    }

    let second_index = first_index + 1;
    if name_of(second) != Some("of") {
        return Some(second_index);
    }

    let kind_index = second_index + 1;
    if tokens.get(kind_index).and_then(name_of).is_none() {
        return Some(first_index);
    }

    let trailing_index = kind_index + 1;
    if tokens.get(trailing_index).and_then(name_of).is_some() {
        Some(trailing_index)
    } else {
        Some(first_index)
    }
}

/// The tokens in a catch header that name its syntax rather than a binding.
///
/// `catch error of <Kind> <name>` binds only `<name>` (or `error` when the
/// kind is written without a name after it). The handler word, `of` and the
/// failure kind are contextual words, so a variable of the same spelling must
/// not answer for them.
fn catch_syntax_indices(tokens: &[Token], catch_index: usize, binder: usize) -> Vec<usize> {
    let mut syntax = Vec::new();
    let handler_index = catch_index + 1;
    let Some(handler) = tokens.get(handler_index) else {
        return syntax;
    };
    if name_of(handler) != Some("error") {
        return syntax;
    }
    if handler_index != binder {
        syntax.push(handler_index);
    }

    let of_index = handler_index + 1;
    let Some(of) = tokens.get(of_index) else {
        return syntax;
    };
    if of.line != handler.line || name_of(of) != Some("of") {
        return syntax;
    }
    if of_index != binder {
        syntax.push(of_index);
    }

    let kind_index = of_index + 1;
    if tokens.get(kind_index).and_then(name_of).is_some() && kind_index != binder {
        syntax.push(kind_index);
    }
    syntax
}

fn lru_cache_decorator_start(tokens: &[Token], close: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut cursor = close;

    loop {
        match &tokens[cursor].kind {
            TokenKind::RightParen => depth += 1,
            TokenKind::LeftParen => {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
                if depth == 0 {
                    let start = cursor.checked_sub(1)?;
                    return matches!(
                        &tokens[start].kind,
                        TokenKind::Identifier(word) if word == "lru_cache"
                    )
                    .then_some(start);
                }
            }
            _ => {}
        }

        if cursor == 0 {
            return None;
        }
        cursor -= 1;
    }
}

/// Whether `index` is the first token on its source line.
fn first_significant_on_line(tokens: &[Token], index: usize) -> bool {
    index == 0 || tokens[index - 1].line != tokens[index].line
}

/// Whether a token can end an expression, so a following `to` is a range's
/// operator rather than the start of a function literal.
fn ends_expression(kind: &TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Identifier(_)
            | TokenKind::Number(_)
            | TokenKind::Text(_)
            | TokenKind::YesNo(_)
            | TokenKind::Nothing
            | TokenKind::This
            | TokenKind::RightParen
            | TokenKind::RightBracket
            | TokenKind::RightBrace
    )
}

/// Whether a token before `to` makes that `to` an operator — a range or an
/// assignment — rather than the start of a one-line function literal.
fn to_is_operator(previous: &TokenKind) -> bool {
    ends_expression(previous) || matches!(previous, TokenKind::Set | TokenKind::Constant)
}

/// Push a fresh lexical scope, remembering the scope that encloses it.
fn push_scope(
    scopes: &mut Vec<HashMap<String, usize>>,
    scope_ids: &mut Vec<usize>,
    scope_parents: &mut Vec<Option<usize>>,
    next_scope_id: &mut usize,
) {
    let parent = scope_ids.last().copied();
    let id = *next_scope_id;
    *next_scope_id += 1;
    scopes.push(HashMap::new());
    scope_ids.push(id);
    scope_parents.push(parent);
}

/// Pop the innermost lexical scope, keeping the root scope in place.
fn pop_scope(scopes: &mut Vec<HashMap<String, usize>>, scope_ids: &mut Vec<usize>) {
    if scopes.len() > 1 {
        scopes.pop();
        scope_ids.pop();
    }
}

/// Closes every one-line literal whose parsed body ends at `index`, before the
/// token there is processed. The token is the `,`/closing bracket that stopped
/// the parser's one-line body, or the newline/EOF that ended the line, so it
/// belongs to the enclosing scope rather than the literal.
fn close_due_frames(
    frames: &mut Vec<Frame>,
    scopes: &mut Vec<HashMap<String, usize>>,
    scope_ids: &mut Vec<usize>,
    runtime_scopes: &mut Vec<usize>,
    runtime_module_frames: &mut Vec<bool>,
    index: usize,
) {
    while matches!(
        frames.last(),
        Some(frame) if frame.closes_at_token == Some(index)
    ) {
        let frame = frames.pop().expect("checked by matches!");
        if frame.opens_scope {
            pop_scope(scopes, scope_ids);
        }
        if pushes_runtime_scope(frame.kind) {
            runtime_scopes.pop();
            runtime_module_frames.pop();
        }
    }
}

/// The identity of the runtime frames enclosing a token, outermost first.
///
/// A `module` body runs inline where its declaration appears, so it is
/// transparent: a write there reaches the runtime frame that contains the
/// declaration. A function, function literal or test body runs later (or not at
/// all), so it adds a frame of its own. A later write applies to a read only
/// when the write's frame path is a prefix of the read's — the write happens in
/// the same frame or one that encloses the read, never in a function the read's
/// frame cannot have entered.
fn runtime_path(
    runtime_scopes: &[usize],
    runtime_module_frames: &[bool],
    scope_ids: &[usize],
) -> Vec<usize> {
    runtime_scopes
        .iter()
        .zip(runtime_module_frames)
        .filter(|(_, is_module)| !**is_module)
        .map(|(position, _)| scope_ids[*position])
        .collect()
}

/// Whether `write` happens in the same runtime frame as `read` or in one that
/// encloses it.
fn runtime_path_encloses(write: &[usize], read: &[usize]) -> bool {
    write.len() <= read.len() && write.iter().zip(read).all(|(left, right)| left == right)
}

/// A member write that may replace a program-declared module's function.
struct ModuleMemberWrite {
    index: usize,
    module: usize,
    name: String,
    path: Vec<usize>,
}

/// Whether a frame is a place where the VM's `set` creates a variable.
///
/// A function call, a function literal, a module and a test body each run in
/// their own variable frame. `if`/`unless` branches, loops, `try`/`catch` and
/// object bodies are analyzer scopes only, so a `set` there writes to the
/// enclosing runtime frame.
fn pushes_runtime_scope(kind: FrameKind) -> bool {
    matches!(
        kind,
        FrameKind::To | FrameKind::Literal | FrameKind::Module | FrameKind::Test
    )
}

/// Whether the name at `index` follows a `.` and therefore names a field of
/// its receiver rather than a lexical binding.
fn is_member_access(tokens: &[Token], index: usize) -> bool {
    index
        .checked_sub(1)
        .and_then(|dot| tokens.get(dot))
        .is_some_and(|token| matches!(token.kind, TokenKind::Dot))
}

/// The member tables a lookup searches, with the parent edges between objects.
struct MemberTables<'a> {
    fields: &'a HashMap<usize, HashMap<String, usize>>,
    methods: &'a HashMap<usize, HashMap<String, usize>>,
    parents: &'a HashMap<usize, usize>,
}

impl MemberTables<'_> {
    /// Whether an object declares `name` in either of its own tables.
    fn declares(&self, object: usize, name: &str) -> bool {
        self.fields
            .get(&object)
            .is_some_and(|fields| fields.contains_key(name))
            || self
                .methods
                .get(&object)
                .is_some_and(|methods| methods.contains_key(name))
    }

    /// Whether a child that inherits from `object` declares `name` on itself.
    ///
    /// A method's body is written inside its own object, but the runtime
    /// receiver can be a child that inherited the method, and the child's
    /// copied table keeps its own entries ahead of every inherited one. When a
    /// descendant declares `name`, the read's answer depends on which object
    /// the method runs against, so navigation must not claim the lexical
    /// object's declaration. Every child of `object` is checked along its own
    /// chain, so a grandchild that overrides through an intermediate object
    /// counts too.
    fn descendant_overrides(&self, object: usize, name: &str) -> bool {
        self.parents.keys().any(|child| {
            let mut current = Some(*child);
            let mut seen = HashSet::new();
            while let Some(current_object) = current {
                if current_object == object || !seen.insert(current_object) {
                    break;
                }
                if self.declares(current_object, name) {
                    return true;
                }
                current = self.parents.get(&current_object).copied();
            }
            false
        })
    }

    /// The nearest entry for `name` in one table, walking the object's parent
    /// chain.
    ///
    /// The runtime copies a parent's fields and methods into the child's own
    /// tables, nearest declaration first, so a child member shadows the
    /// same-named member of every ancestor. Navigation walks the chain instead
    /// of copying it, which keeps the declaration span on the object that
    /// actually wrote it. A malformed `extends` cycle stops at the first
    /// object it revisits rather than looping forever.
    fn nearest_in(
        &self,
        table: &HashMap<usize, HashMap<String, usize>>,
        receiver: usize,
        name: &str,
    ) -> Option<usize> {
        let mut current = Some(receiver);
        let mut seen = HashSet::new();
        while let Some(object) = current {
            if !seen.insert(object) {
                return None;
            }
            if let Some(binding) = table.get(&object).and_then(|members| members.get(name)) {
                return Some(*binding);
            }
            current = self.parents.get(&object).copied();
        }
        None
    }

    /// The nearest entry for `name` over both tables, one table at a time. A
    /// call searches the methods first and a read the fields, so the other
    /// table only answers for objects that do not declare the name in the
    /// preferred one.
    fn nearest(&self, receiver: usize, name: &str, prefer_methods: bool) -> Option<usize> {
        let (first, second) = if prefer_methods {
            (self.methods, self.fields)
        } else {
            (self.fields, self.methods)
        };
        self.nearest_in(first, receiver, name)
            .or_else(|| self.nearest_in(second, receiver, name))
    }
}

/// The binding a member name resolves to through `receiver`.
///
/// A member followed by `(` is a method call, so the object's methods answer
/// before its fields; a bare member is a field access. Falling back to the
/// other keeps a method name readable as a value and a callable field callable.
/// Both searches walk the `extends` chain, because the runtime's object tables
/// already contain the inherited members.
///
/// When the receiver is `this`, the runtime receiver is whichever object the
/// method actually runs against, and a descendant that overrides `name`
/// shadows the lexical object's answer — so a `this` read that could land on
/// an override answers nothing rather than the ancestor's declaration.
fn member_binding(
    tokens: &[Token],
    index: usize,
    receiver: usize,
    receiver_is_this: bool,
    tables: &MemberTables<'_>,
    name: &str,
) -> Option<usize> {
    if receiver_is_this && tables.descendant_overrides(receiver, name) {
        return None;
    }
    tables.nearest(receiver, name, call_follows(tokens, index))
}

/// Whether the name at `index` is followed by `(`, so it is a call rather than
/// a bare read of the same spelling.
fn call_follows(tokens: &[Token], index: usize) -> bool {
    matches!(
        tokens.get(index + 1).map(|token| &token.kind),
        Some(TokenKind::LeftParen)
    )
}

/// The function binding a called name resolves to in `scope_id`, if the scope
/// holds one alongside the plain name. A method and a field can share a
/// spelling; the scope's plain entry keeps the first declaration, so a call has
/// to look for the method explicitly, mirroring [`member_binding`].
///
/// Only a function declared after the scope's current entry and before the use
/// can win. A later `set` replaces an earlier function in the same runtime
/// slot, so a call after that `set` must not fall back to the shadowed
/// function, nor may it jump forward to a function written after the call. A
/// method declared after a plain variable is still later and earlier than the
/// use, so `name()` keeps resolving to it.
fn scope_function_binding(
    name: &str,
    first: usize,
    scope_id: usize,
    use_location: Location,
    bindings: &[Binding],
    binding_scopes: &[usize],
) -> Option<usize> {
    if bindings[first].lexical && bindings[first].kind == DefinitionKind::Function {
        return Some(first);
    }
    binding_scopes
        .iter()
        .enumerate()
        .find(|(id, scope)| {
            *id > first
                && **scope == scope_id
                && bindings[*id].lexical
                && bindings[*id].name == name
                && bindings[*id].kind == DefinitionKind::Function
                && precedes(bindings[*id].location, use_location)
        })
        .map(|(id, _)| id)
}

/// Whether `declaration` is written before `use` in source order.
fn precedes(declaration: Location, use_location: Location) -> bool {
    (declaration.line, declaration.column) < (use_location.line, use_location.column)
}

/// The lexical binding a bare name resolves to in `scope_id`.
///
/// The scope's entry is the latest declaration of the spelling, because a
/// `set`, `constant` or `to` replaces a same-named binding of another kind.
/// That is exactly what the runtime's slot holds when the read executes, so a
/// bare read answers the entry rather than preferring a function or a variable
/// by kind. A method is call-only: it leaves an earlier plain binding in place,
/// and when no earlier binding exists the entry answers no bare read at all,
/// matching the analyzer's `Unknown variable` for a bare method name.
fn scope_plain_binding(
    name: &str,
    first: usize,
    scope_id: usize,
    bindings: &[Binding],
    binding_scopes: &[usize],
) -> Option<usize> {
    if bindings[first].lexical && !bindings[first].call_only {
        return Some(first);
    }
    binding_scopes
        .iter()
        .enumerate()
        .find(|(id, scope)| {
            **scope == scope_id
                && bindings[*id].lexical
                && !bindings[*id].call_only
                && bindings[*id].name == name
        })
        .map(|(id, _)| id)
}

/// The token index where a one-line literal body starting at `start` ends.
///
/// The parser's one-line body keeps reading statements while the next token can
/// begin one, so it stops at a `,`, `)`, `]` or `}` that closes the enclosing
/// expression, or at the newline/EOF that ends the line. Nested brackets are
/// skipped so a delimiter inside the body's own expression is not mistaken for
/// the literal's end.
fn literal_body_end(tokens: &[Token], start: usize, open_line: usize) -> usize {
    let mut depth = 0usize;
    let mut cursor = start;

    while let Some(token) = tokens.get(cursor) {
        if token.line != open_line || matches!(token.kind, TokenKind::Newline | TokenKind::Eof) {
            return cursor;
        }
        match token.kind {
            TokenKind::LeftParen | TokenKind::LeftBracket | TokenKind::LeftBrace => depth += 1,
            TokenKind::RightParen | TokenKind::RightBracket | TokenKind::RightBrace => {
                if depth == 0 {
                    return cursor;
                }
                depth -= 1;
            }
            TokenKind::Comma if depth == 0 => return cursor,
            _ => {}
        }
        cursor += 1;
    }

    cursor
}

fn is_builtin_name(name: &str) -> bool {
    BUILTIN_NAMES.contains(&name) || MODULES.contains(&name) || name == "PI" || name == "E"
}

/// Whether a name written before its declaration may resolve to `declaration`.
///
/// A builtin answers until the program claims its name. A declaration earlier
/// in the file has already done so, and a `constant` or `import` claims the
/// name wherever it is written. A later first `set`, loop variable or function
/// does not pre-claim it, so leaving the use unbound lets the builtin fallback
/// answer exactly as the analyzer does.
fn deferred_builtin_binds_to(declaration: &Binding, token: &Token) -> bool {
    (declaration.location.line, declaration.location.column) <= (token.line, token.column)
        || matches!(
            declaration.kind,
            DefinitionKind::Constant | DefinitionKind::Import
        )
}

/// Walks the token stream once, at token level, building lexical scopes.
///
/// A declaration's shape is recognised from the tokens around it rather than
/// from the AST because the AST does not keep a span for a `set` name, a
/// parameter or an imported alias — and a navigation answer is exactly those
/// spans. When `stop` is given, analysis stops before the token at that source
/// offset and records the names visible there.
fn analyze(source: &str, stop: Option<usize>) -> Result<Analysis, String> {
    let tokens = Lexer::tokenize(source).map_err(|error| match error.span() {
        Some(span) => format!("{}: {}", span, error.message()),
        None => error.message().to_string(),
    })?;
    let map = SourceMap::new(source);
    let mut bindings: Vec<Binding> = Vec::new();
    let mut token_binding: Vec<Option<usize>> = vec![None; tokens.len()];
    let mut declarations: HashSet<usize> = HashSet::new();
    let mut scopes: Vec<HashMap<String, usize>> = vec![HashMap::new()];
    let mut scope_ids: Vec<usize> = vec![0];
    let mut scope_parents: Vec<Option<usize>> = vec![None];
    let mut next_scope_id = 1usize;
    let mut binding_scopes: Vec<usize> = Vec::new();
    let mut token_scope: Vec<usize> = vec![0; tokens.len()];
    let mut type_names: HashSet<usize> = HashSet::new();
    let mut syntax_names: HashSet<usize> = HashSet::new();
    let mut member_names: HashSet<usize> = HashSet::new();
    let mut object_fields: HashMap<usize, HashMap<String, usize>> = HashMap::new();
    let mut object_methods: HashMap<usize, HashMap<String, usize>> = HashMap::new();
    // A child binding to its parent binding, filled after the deferred name
    // pass so the `extends` name has the binding the walk gave it.
    let mut object_parents: HashMap<usize, usize> = HashMap::new();
    let mut this_objects: HashMap<usize, usize> = HashMap::new();
    let mut module_aliases: HashMap<String, String> = HashMap::new();
    // The top-level names each live `module … end` body declares, and the last
    // `export` it writes. Both are keyed by the module's binding; the names are
    // filtered by the export when the module closes.
    let mut module_declared: HashMap<usize, Vec<String>> = HashMap::new();
    let mut module_functions: HashMap<usize, HashMap<String, usize>> = HashMap::new();
    let mut module_exports: HashMap<usize, (Vec<String>, bool)> = HashMap::new();
    let mut declared_module_members: HashMap<String, Vec<String>> = HashMap::new();
    // The same published names, keyed by the module binding so a same-named
    // declaration cannot answer for an earlier module's import alias.
    let mut declared_module_members_by_binding: HashMap<usize, Vec<String>> = HashMap::new();
    // The published functions of each module, keyed by the module's binding so
    // two same-named declarations cannot overwrite each other. An import records
    // the binding it names at the import site, so a later same-named module
    // cannot change what an earlier import refers to.
    let mut declared_module_functions: HashMap<usize, HashMap<String, usize>> = HashMap::new();
    // Member tokens that are the target of a `set Receiver.member` write, each
    // mapped to the runtime frame path where the write happens. A write
    // replaces the module member's value, so the deferred pass must drop the
    // stale published function before resolving the write or any later read —
    // but only for reads in the same runtime frame (or one it encloses), since
    // a write inside an uncalled function never touches the module table.
    // Object writes keep their member binding and are unaffected.
    let mut member_writes: HashMap<usize, Vec<usize>> = HashMap::new();
    // The runtime frame path of every member token, recorded when the walk
    // reaches it, so a write can be compared with each read it might affect.
    let mut member_runtime_paths: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut import_module_bindings: HashMap<usize, usize> = HashMap::new();
    // The latest module binding for a name, used only by an import written
    // before its module is declared: at that point there is no binding to
    // record, so the deferred pass falls back to this table.
    let mut module_name_bindings: HashMap<String, usize> = HashMap::new();
    let mut frames: Vec<Frame> = Vec::new();
    // Stack positions of the live `test` body scopes. A `test` runs in an
    // isolated VM, so a name its first `set` creates belongs to that VM's
    // program scope — the test body — rather than to the file's root scope.
    let mut test_scopes: Vec<usize> = Vec::new();
    // Stack positions of the live `module` body scopes. The VM binds a first
    // `set` inside a module to the module's own scope rather than to a global,
    // so navigation must put that binding there too.
    let mut module_scopes: Vec<usize> = Vec::new();
    // Stack positions of the live frames where the VM's `set` creates a
    // variable: the program, a function call, a function literal, a module and
    // a test body. Block scopes are analyzer-only, so a `set` there writes to
    // the innermost entry of this stack.
    let mut runtime_scopes: Vec<usize> = vec![0];
    // One flag per live runtime frame: whether it is a `module` body, which
    // runs inline and is therefore transparent for write reachability.
    let mut runtime_module_frames: Vec<bool> = vec![false];
    let mut visible = None;
    let mut last_significant_line: Option<usize> = None;
    let mut last_significant: Option<usize> = None;
    // Names the walk already looked up in a live scope, even when the answer
    // was `None` because a later declaration shadows the only function. The
    // deferred pass must not resurrect such a shadowed binding.
    let mut looked_up: HashSet<usize> = HashSet::new();
    let mut index = 0;

    while index < tokens.len() {
        close_due_frames(
            &mut frames,
            &mut scopes,
            &mut scope_ids,
            &mut runtime_scopes,
            &mut runtime_module_frames,
            index,
        );
        token_scope[index] = scope_ids.last().copied().unwrap_or(0);

        if let Some(stop) = stop {
            let token = &tokens[index];
            if map.offset(token.line, token.column) >= stop {
                visible = Some(visible_names(&scopes));
                break;
            }
        }

        let token = tokens[index].clone();
        match &token.kind {
            TokenKind::Newline | TokenKind::Eof => {}
            TokenKind::End => {
                if let Some(frame) = frames.pop() {
                    if frame.opens_scope || frame.catch_scope {
                        pop_scope(&mut scopes, &mut scope_ids);
                    }
                    if frame.kind == FrameKind::Test {
                        test_scopes.pop();
                    }
                    if frame.kind == FrameKind::Module {
                        module_scopes.pop();
                        if let Some(module) = frame.module {
                            let declared = module_declared.remove(&module).unwrap_or_default();
                            let members = match module_exports.remove(&module) {
                                // No `export` publishes nothing, exactly as the
                                // runtime's `declare_module` reads it.
                                None => Vec::new(),
                                Some((_, true)) => declared,
                                Some((names, false)) => {
                                    let declared: HashSet<&str> =
                                        declared.iter().map(String::as_str).collect();
                                    names
                                        .into_iter()
                                        .filter(|name| declared.contains(name.as_str()))
                                        .collect()
                                }
                            };
                            let mut members = members;
                            members.sort();
                            members.dedup();
                            let name = bindings[module].name.clone();
                            // Only a published function is reachable through
                            // `Module.name()`, matching the runtime's module
                            // table, so navigation keeps only those bindings.
                            let published: HashMap<String, usize> = module_functions
                                .remove(&module)
                                .unwrap_or_default()
                                .into_iter()
                                .filter(|(member, _)| members.contains(member))
                                .collect();
                            declared_module_functions.insert(module, published);
                            declared_module_members_by_binding.insert(module, members.clone());
                            module_name_bindings.insert(name.clone(), module);
                            declared_module_members.insert(name, members);
                        }
                    }
                    if pushes_runtime_scope(frame.kind) {
                        runtime_scopes.pop();
                        runtime_module_frames.pop();
                    }
                }
            }
            TokenKind::Until => {
                if matches!(frames.last(), Some(frame) if frame.kind == FrameKind::Repeat) {
                    let frame = frames.pop().expect("checked by matches!");
                    if frame.opens_scope {
                        pop_scope(&mut scopes, &mut scope_ids);
                    }
                }
            }
            TokenKind::If => {
                let continues_else = last_significant.is_some_and(|previous| {
                    tokens[previous].line == token.line
                        && matches!(tokens[previous].kind, TokenKind::Else)
                });
                if !continues_else {
                    frames.push(Frame::scoped(FrameKind::If));
                    push_scope(
                        &mut scopes,
                        &mut scope_ids,
                        &mut scope_parents,
                        &mut next_scope_id,
                    );
                }
            }
            // A branch gets a scope of its own, matching the analyzer, which
            // pushes and pops one around each branch. `else if` keeps the
            // frame the `if` opened: the `else` below closes the then branch's
            // scope and opens the one the rest of the chain shares.
            TokenKind::Else => {
                if let Some(frame) = frames.last_mut() {
                    if frame.kind == FrameKind::If && frame.opens_scope {
                        pop_scope(&mut scopes, &mut scope_ids);
                        push_scope(
                            &mut scopes,
                            &mut scope_ids,
                            &mut scope_parents,
                            &mut next_scope_id,
                        );
                    }
                }
            }
            // The analyzer pushes and pops a scope around an `unless` body, so
            // a function, object or import declared there does not outlive the
            // body. Navigation opens the same scope; a `set` still writes
            // through to the program scope it lands in at runtime.
            TokenKind::Unless => {
                frames.push(Frame::scoped(FrameKind::Unless));
                push_scope(
                    &mut scopes,
                    &mut scope_ids,
                    &mut scope_parents,
                    &mut next_scope_id,
                );
            }
            TokenKind::While => {
                frames.push(Frame::scoped(FrameKind::While));
                push_scope(
                    &mut scopes,
                    &mut scope_ids,
                    &mut scope_parents,
                    &mut next_scope_id,
                );
            }
            TokenKind::Repeat => {
                frames.push(Frame::scoped(FrameKind::Repeat));
                push_scope(
                    &mut scopes,
                    &mut scope_ids,
                    &mut scope_parents,
                    &mut next_scope_id,
                );
            }
            TokenKind::For => {
                frames.push(Frame::scoped(FrameKind::For));
                push_scope(
                    &mut scopes,
                    &mut scope_ids,
                    &mut scope_parents,
                    &mut next_scope_id,
                );
                let mut cursor = index + 1;
                if matches!(
                    tokens.get(cursor).map(|token| &token.kind),
                    Some(TokenKind::Each)
                ) {
                    cursor += 1;
                }
                if let Some(name) = tokens.get(cursor).and_then(name_of) {
                    let location = Location::of_name(&tokens[cursor], name);
                    let binding = declare(
                        &mut scopes,
                        &mut bindings,
                        &mut binding_scopes,
                        *scope_ids.last().expect("the root scope always exists"),
                        name,
                        location,
                        DefinitionKind::LoopVariable,
                    );
                    mark_declaration(&mut token_binding, &mut declarations, cursor, binding);
                }
            }
            TokenKind::Object => {
                let mut object = None;
                if let Some(name) = tokens.get(index + 1).and_then(name_of) {
                    let location = Location::of_name(&tokens[index + 1], name);
                    let binding = declare(
                        &mut scopes,
                        &mut bindings,
                        &mut binding_scopes,
                        *scope_ids.last().expect("the root scope always exists"),
                        name,
                        location,
                        DefinitionKind::Object,
                    );
                    mark_declaration(&mut token_binding, &mut declarations, index + 1, binding);
                    object = Some(binding);
                }
                let mut frame = Frame::scoped(FrameKind::Object);
                frame.object = object;
                frames.push(frame);
                push_scope(
                    &mut scopes,
                    &mut scope_ids,
                    &mut scope_parents,
                    &mut next_scope_id,
                );
            }
            TokenKind::Module => {
                let mut module = None;
                if let Some(name) = tokens.get(index + 1).and_then(name_of) {
                    let location = Location::of_name(&tokens[index + 1], name);
                    let binding = declare_module_binding(
                        &mut scopes,
                        &mut bindings,
                        &mut binding_scopes,
                        *scope_ids.last().expect("the root scope always exists"),
                        name,
                        location,
                    );
                    mark_declaration(&mut token_binding, &mut declarations, index + 1, binding);
                    module = Some(binding);
                }
                let mut frame = Frame::scoped(FrameKind::Module);
                frame.module = module;
                frames.push(frame);
                push_scope(
                    &mut scopes,
                    &mut scope_ids,
                    &mut scope_parents,
                    &mut next_scope_id,
                );
                module_scopes.push(scopes.len() - 1);
                runtime_scopes.push(scopes.len() - 1);
                runtime_module_frames.push(true);
            }
            // The test harness parses each `test "..."` body on its own and
            // runs it in an isolated VM, so two tests never share a binding.
            // A test body gets its own scope here for the same reason; it can
            // still read the enclosing scope, but a declaration it writes does
            // not reach the next test or the file after `end`.
            TokenKind::Test => {
                frames.push(Frame::scoped(FrameKind::Test));
                push_scope(
                    &mut scopes,
                    &mut scope_ids,
                    &mut scope_parents,
                    &mut next_scope_id,
                );
                test_scopes.push(scopes.len() - 1);
                runtime_scopes.push(scopes.len() - 1);
                runtime_module_frames.push(false);
            }
            TokenKind::Try => frames.push(Frame::block(FrameKind::Try)),
            // A `catch` body runs through `run_catch_body`, which pushes a
            // scope for its binder. The `finally` body does not: it runs
            // through `run_finally_body`/`run_block`, so it belongs to the
            // enclosing scope and must not see the catch binder. Close the
            // catch scope as soon as `finally` starts.
            TokenKind::Finally => {
                if let Some(frame) = frames.last_mut() {
                    if frame.kind == FrameKind::Try && frame.catch_scope {
                        frame.catch_scope = false;
                        pop_scope(&mut scopes, &mut scope_ids);
                    }
                }
            }
            TokenKind::Catch => {
                let opens_scope = match frames.last_mut() {
                    Some(frame) if frame.kind == FrameKind::Try && !frame.catch_scope => {
                        frame.catch_scope = true;
                        true
                    }
                    _ => false,
                };
                if opens_scope {
                    push_scope(
                        &mut scopes,
                        &mut scope_ids,
                        &mut scope_parents,
                        &mut next_scope_id,
                    );
                }
                if let Some(binder) = catch_binder_index(&tokens, index) {
                    for syntax in catch_syntax_indices(&tokens, index, binder) {
                        syntax_names.insert(syntax);
                    }
                    let name = name_of(&tokens[binder]).expect("the binder is a name");
                    let location = Location::of_name(&tokens[binder], name);
                    let binding = declare(
                        &mut scopes,
                        &mut bindings,
                        &mut binding_scopes,
                        *scope_ids.last().expect("the root scope always exists"),
                        name,
                        location,
                        DefinitionKind::Catch,
                    );
                    mark_declaration(&mut token_binding, &mut declarations, binder, binding);
                }
            }
            TokenKind::To => {
                let first_on_line = last_significant_line != Some(token.line);
                let mut cursor = index + 1;
                if matches!(
                    tokens.get(cursor).map(|token| &token.kind),
                    Some(TokenKind::Can)
                ) {
                    cursor += 1;
                }

                let decorated = last_significant.is_some_and(|previous| {
                    let start = match &tokens[previous].kind {
                        TokenKind::Identifier(word) if word == "lru_cache" => Some(previous),
                        TokenKind::RightParen => lru_cache_decorator_start(&tokens, previous),
                        _ => None,
                    };
                    start.is_some_and(|start| {
                        tokens[start].line == token.line
                            && first_significant_on_line(&tokens, start)
                    })
                });
                let is_declaration =
                    tokens.get(cursor).and_then(name_of).is_some() && (first_on_line || decorated);

                if is_declaration {
                    let name = match &tokens[cursor].kind {
                        TokenKind::Identifier(name) => name.clone(),
                        _ => unreachable!("checked by name_of"),
                    };
                    let location = Location::of_name(&tokens[cursor], &name);
                    let method_owner = frames.last().and_then(|frame| frame.object);
                    let module_owner = frames.last().and_then(|frame| frame.module);
                    let scope_id = *scope_ids.last().expect("the root scope always exists");
                    let binding = if method_owner.is_some() {
                        declare_fresh(
                            &mut scopes,
                            &mut bindings,
                            &mut binding_scopes,
                            scope_id,
                            &name,
                            location,
                            DefinitionKind::Function,
                        )
                    } else {
                        declare_function_binding(
                            &mut scopes,
                            &mut bindings,
                            &mut binding_scopes,
                            scope_id,
                            &name,
                            location,
                        )
                    };
                    mark_declaration(&mut token_binding, &mut declarations, cursor, binding);
                    if let Some(owner) = method_owner {
                        object_methods
                            .entry(owner)
                            .or_default()
                            .insert(name.clone(), binding);
                    }
                    if let Some(owner) = module_owner {
                        module_declared.entry(owner).or_default().push(name.clone());
                        module_functions
                            .entry(owner)
                            .or_default()
                            .insert(name.clone(), binding);
                    }
                    frames.push(Frame::scoped(FrameKind::To));
                    push_scope(
                        &mut scopes,
                        &mut scope_ids,
                        &mut scope_parents,
                        &mut next_scope_id,
                    );
                    runtime_scopes.push(scopes.len() - 1);
                    runtime_module_frames.push(false);
                    declare_params(
                        &tokens,
                        cursor + 1,
                        &mut scopes,
                        &mut bindings,
                        &mut binding_scopes,
                        *scope_ids.last().expect("the root scope always exists"),
                        &mut DeclarationSink {
                            token_binding: &mut token_binding,
                            declarations: &mut declarations,
                            type_names: &mut type_names,
                        },
                    );
                } else if !last_significant
                    .is_some_and(|previous| to_is_operator(&tokens[previous].kind))
                {
                    let mut frame = Frame::scoped(FrameKind::Literal);
                    push_scope(
                        &mut scopes,
                        &mut scope_ids,
                        &mut scope_parents,
                        &mut next_scope_id,
                    );
                    runtime_scopes.push(scopes.len() - 1);
                    runtime_module_frames.push(false);
                    let mut body = index + 1;
                    if matches!(
                        tokens.get(body).map(|token| &token.kind),
                        Some(TokenKind::LeftParen)
                    ) {
                        body = declare_params(
                            &tokens,
                            body,
                            &mut scopes,
                            &mut bindings,
                            &mut binding_scopes,
                            *scope_ids.last().expect("the root scope always exists"),
                            &mut DeclarationSink {
                                token_binding: &mut token_binding,
                                declarations: &mut declarations,
                                type_names: &mut type_names,
                            },
                        ) + 1;
                    }
                    if let Some(next) = tokens.get(body) {
                        if next.line == token.line
                            && !matches!(next.kind, TokenKind::Newline | TokenKind::Eof)
                        {
                            frame.closes_at_token =
                                Some(literal_body_end(&tokens, body, token.line));
                        }
                    }
                    frames.push(frame);
                }
            }
            TokenKind::Import => {
                let mut cursor = index + 1;
                let mut module: Option<String> = None;
                let mut source_binding: Option<usize> = None;
                while cursor < tokens.len() {
                    match tokens[cursor].kind.clone() {
                        TokenKind::Newline | TokenKind::Eof => break,
                        TokenKind::Comma => {
                            module = None;
                            source_binding = None;
                            cursor += 1;
                        }
                        TokenKind::Identifier(name) => {
                            let location = Location::of_name(&tokens[cursor], &name);
                            let binding = declare(
                                &mut scopes,
                                &mut bindings,
                                &mut binding_scopes,
                                *scope_ids.last().expect("the root scope always exists"),
                                &name,
                                location,
                                DefinitionKind::Import,
                            );
                            mark_declaration(
                                &mut token_binding,
                                &mut declarations,
                                cursor,
                                binding,
                            );
                            module_aliases
                                .entry(name.clone())
                                .or_insert_with(|| name.clone());
                            source_binding = module_name_bindings.get(&name).copied();
                            if let Some(module_binding) = source_binding {
                                import_module_bindings.insert(binding, module_binding);
                            }
                            module = Some(name);
                            cursor += 1;
                        }
                        TokenKind::As | TokenKind::To => {
                            cursor += 1;
                            if let Some(name) = tokens.get(cursor).and_then(name_of) {
                                let location = Location::of_name(&tokens[cursor], name);
                                let binding = declare(
                                    &mut scopes,
                                    &mut bindings,
                                    &mut binding_scopes,
                                    *scope_ids.last().expect("the root scope always exists"),
                                    name,
                                    location,
                                    DefinitionKind::Import,
                                );
                                mark_declaration(
                                    &mut token_binding,
                                    &mut declarations,
                                    cursor,
                                    binding,
                                );
                                if let Some(module_binding) = source_binding.take() {
                                    import_module_bindings.insert(binding, module_binding);
                                }
                                if let Some(source) = module.take() {
                                    module_aliases.insert(name.to_string(), source);
                                }
                                cursor += 1;
                            }
                        }
                        _ => cursor += 1,
                    }
                }
                index = cursor;
                continue;
            }
            TokenKind::Set | TokenKind::Constant => {
                let kind = if matches!(token.kind, TokenKind::Constant) {
                    DefinitionKind::Constant
                } else {
                    DefinitionKind::Variable
                };
                let mut cursor = index + 1;
                while let Some(name) = tokens.get(cursor).and_then(name_of) {
                    // `set Person.name to …` writes a member of an existing
                    // receiver (an object or a module), so the name is a use of
                    // that binding, not a fresh variable. Leaving the token
                    // unanswered lets the ordinary identifier pass resolve it
                    // and the member pass resolve the field or method.
                    if matches!(
                        tokens.get(cursor + 1).map(|token| &token.kind),
                        Some(TokenKind::Dot)
                    ) {
                        if tokens.get(cursor + 2).and_then(name_of).is_some() {
                            let path =
                                runtime_path(&runtime_scopes, &runtime_module_frames, &scope_ids);
                            member_writes.insert(cursor + 2, path);
                        }
                        // A typed target's annotation is a type, not a use of
                        // the variable or parameter that shares its spelling.
                        // The member walk breaks here, so consume the type
                        // segment before it does.
                        if matches!(
                            tokens.get(cursor + 3).map(|token| &token.kind),
                            Some(TokenKind::Colon)
                        ) {
                            skip_type_expression(&tokens, cursor + 4, true, &mut type_names);
                        }
                        break;
                    }
                    if let Some(owner) = frames.last().and_then(|frame| frame.module) {
                        module_declared
                            .entry(owner)
                            .or_default()
                            .push(name.to_string());
                    }
                    // A `set` or `constant` after a same-named `to` replaces the
                    // module member's value, so the function binding no longer
                    // answers a call through the module. The replacement may be
                    // written in a nested block, which opens an analyzer scope
                    // but no runtime frame, so find the module whose scope the
                    // write reaches by walking to the innermost frame that
                    // creates variables rather than looking only at the top.
                    if let Some(owner) = frames
                        .iter()
                        .rev()
                        .find(|frame| pushes_runtime_scope(frame.kind))
                        .and_then(|frame| frame.module)
                    {
                        if let Some(functions) = module_functions.get_mut(&owner) {
                            functions.remove(name);
                        }
                    }
                    // `set` writes through to an enclosing binding when one
                    // exists (the interpreter's `set_var` does the same), so
                    // reuse it instead of declaring a fresh inner name. A name
                    // no scope binds is the program's own, because that is
                    // where the VM writes the first assignment — inside a
                    // function or a loop it still outlives the block, exactly
                    // as `Analyzer::declare_assigned` models it.
                    //
                    // A `constant` always lands in the program scope too: the
                    // VM binds it to a global whatever block declares it, and
                    // the analyzer does the same.
                    //
                    // Inside a `test`, that program scope is the test body's
                    // own scope: the harness runs each body in an isolated VM,
                    // so a name it creates must not reach the next test or the
                    // file after `end`. Inside a `module`, it is the module
                    // body's own scope: the VM binds a first assignment there
                    // rather than in the importing file's globals.
                    let existing = if matches!(token.kind, TokenKind::Set) {
                        scopes
                            .iter()
                            .rev()
                            .find_map(|scope| scope.get(name).copied())
                    } else {
                        None
                    };
                    if let Some(binding) =
                        existing.filter(|&binding| set_reuses_binding(bindings[binding].kind))
                    {
                        token_binding[cursor] = Some(binding);
                    } else if existing.is_some() {
                        // The name currently resolves to a declaration a `set`
                        // cannot write through — a function, constant, import,
                        // object or module. The runtime's `set` shadows it with
                        // a variable in the innermost frame that creates
                        // variables (a block scope writes through to its
                        // function or the program). The earlier declaration
                        // keeps its uses; this one starts a new reference set.
                        let position = runtime_scopes.last().copied().unwrap_or(0);
                        let location = Location::of_name(&tokens[cursor], name);
                        let binding = declare_replacing_binding(
                            &mut scopes,
                            &mut bindings,
                            &mut binding_scopes,
                            scope_ids[position],
                            position,
                            name,
                            location,
                        );
                        mark_declaration(&mut token_binding, &mut declarations, cursor, binding);
                    } else {
                        let location = Location::of_name(&tokens[cursor], name);
                        // A constant is a global whatever block declares it, so
                        // only an isolated `test` moves its program scope. A
                        // first `set` follows the innermost module or test body.
                        let program_position = if matches!(token.kind, TokenKind::Set) {
                            test_scopes
                                .last()
                                .copied()
                                .max(module_scopes.last().copied())
                        } else {
                            test_scopes.last().copied()
                        };
                        let (program_scopes, program_id): (&mut [HashMap<String, usize>], usize) =
                            match program_position {
                                Some(position) => (&mut scopes[..=position], scope_ids[position]),
                                None => (&mut scopes[..1], 0),
                            };
                        let binding = if kind == DefinitionKind::Constant {
                            declare_constant_binding(
                                program_scopes,
                                &mut bindings,
                                &mut binding_scopes,
                                program_id,
                                name,
                                location,
                            )
                        } else {
                            declare(
                                program_scopes,
                                &mut bindings,
                                &mut binding_scopes,
                                program_id,
                                name,
                                location,
                                kind,
                            )
                        };
                        mark_declaration(&mut token_binding, &mut declarations, cursor, binding);
                    }
                    cursor += 1;

                    if matches!(
                        tokens.get(cursor).map(|token| &token.kind),
                        Some(TokenKind::Colon)
                    ) {
                        cursor = skip_type_expression(&tokens, cursor + 1, true, &mut type_names);
                    }

                    match tokens.get(cursor).map(|token| &token.kind) {
                        Some(TokenKind::Comma) => cursor += 1,
                        _ => break,
                    }
                }
            }
            // A module's last `export` decides which of its top-level names it
            // publishes, exactly as `parser::module_exports` and the runtime's
            // `declare_module` read it. Outside a module declaration an
            // `export` publishes nothing and is ignored.
            TokenKind::Export => {
                if let Some(owner) = frames.last().and_then(|frame| frame.module) {
                    let mut names = Vec::new();
                    let mut all = false;
                    let mut cursor = index + 1;
                    while cursor < tokens.len() {
                        match &tokens[cursor].kind {
                            TokenKind::Newline | TokenKind::Eof | TokenKind::End => break,
                            TokenKind::Identifier(word) if word == "all" => {
                                all = true;
                                cursor += 1;
                            }
                            TokenKind::Identifier(word) => {
                                names.push(word.clone());
                                cursor += 1;
                            }
                            TokenKind::Comma => cursor += 1,
                            _ => cursor += 1,
                        }
                    }
                    module_exports.insert(owner, (names, all));
                }
            }
            TokenKind::Has => {
                let first_on_line = last_significant_line != Some(token.line);
                let in_object = matches!(
                    frames.last(),
                    Some(frame) if frame.kind == FrameKind::Object
                );
                if first_on_line && in_object {
                    if let Some(name) = tokens.get(index + 1).and_then(name_of) {
                        let location = Location::of_name(&tokens[index + 1], name);
                        let binding = declare_member(
                            &mut bindings,
                            &mut binding_scopes,
                            *scope_ids.last().expect("the root scope always exists"),
                            name,
                            location,
                        );
                        mark_declaration(&mut token_binding, &mut declarations, index + 1, binding);
                        if let Some(owner) = frames.last().and_then(|frame| frame.object) {
                            object_fields
                                .entry(owner)
                                .or_default()
                                .insert(name.to_string(), binding);
                        }
                        if tokens.get(index + 2).and_then(name_of) == Some("default") {
                            syntax_names.insert(index + 2);
                        }
                    }
                }
            }
            TokenKind::This => {
                if let Some(object) = frames.iter().rev().find_map(|frame| frame.object) {
                    this_objects.insert(index, object);
                }
            }
            TokenKind::Identifier(name) if !declarations.contains(&index) => {
                if type_names.contains(&index) || syntax_names.contains(&index) {
                    // A type annotation or a contextual word names something
                    // other than the variable of the same spelling that
                    // happens to be in scope.
                } else if is_member_access(&tokens, index) {
                    member_names.insert(index);
                    member_runtime_paths.insert(
                        index,
                        runtime_path(&runtime_scopes, &runtime_module_frames, &scope_ids),
                    );
                    let receiver = index
                        .checked_sub(2)
                        .and_then(|receiver| tokens.get(receiver))
                        .and_then(|token| match token.kind {
                            TokenKind::This => frames.iter().rev().find_map(|frame| frame.object),
                            _ => token_binding[index - 2],
                        });
                    let receiver_is_this = index
                        .checked_sub(2)
                        .and_then(|receiver| tokens.get(receiver))
                        .is_some_and(|token| matches!(token.kind, TokenKind::This));
                    if let Some(receiver) = receiver {
                        token_binding[index] = member_binding(
                            &tokens,
                            index,
                            receiver,
                            receiver_is_this,
                            &MemberTables {
                                fields: &object_fields,
                                methods: &object_methods,
                                parents: &object_parents,
                            },
                            name,
                        );
                    }
                } else {
                    let is_call = call_follows(&tokens, index);
                    for (scope, scope_id) in scopes.iter().zip(scope_ids.iter()).rev() {
                        let Some(&first) = scope.get(name) else {
                            continue;
                        };
                        token_binding[index] = if is_call {
                            scope_function_binding(
                                name,
                                first,
                                *scope_id,
                                Location::of_name(&tokens[index], name),
                                &bindings,
                                &binding_scopes,
                            )
                        } else {
                            scope_plain_binding(name, first, *scope_id, &bindings, &binding_scopes)
                        };
                        looked_up.insert(index);
                        break;
                    }
                }
            }
            _ => {}
        }

        if !matches!(token.kind, TokenKind::Newline | TokenKind::Eof) {
            last_significant_line = Some(token.line);
            last_significant = Some(index);
        }
        index += 1;
    }

    // A name written before its declaration, such as a body that calls a
    // function declared below it, has no earlier scope entry to resolve
    // against. Resolve those remaining names against the declarations each
    // enclosing scope ended up with, so navigation sees the declaration the
    // program will have when that code runs.
    if stop.is_none() {
        let mut scope_decls: Vec<HashMap<String, usize>> = vec![HashMap::new(); next_scope_id];
        for (binding, scope) in binding_scopes.iter().enumerate() {
            if !bindings[binding].lexical {
                continue;
            }
            scope_decls[*scope]
                .entry(bindings[binding].name.clone())
                .or_insert(binding);
        }

        for index in 0..tokens.len() {
            let Some(name) = name_of(&tokens[index]) else {
                continue;
            };
            if declarations.contains(&index)
                || type_names.contains(&index)
                || syntax_names.contains(&index)
                || member_names.contains(&index)
            {
                continue;
            }

            let is_call = call_follows(&tokens, index);
            if token_binding[index].is_some() || looked_up.contains(&index) {
                // A name resolved during the walk keeps that binding, including
                // a shadowed call that resolved to nothing. This pass exists
                // for names written before their declaration; re-resolving
                // every name would replace an earlier binding with a later
                // same-spelling declaration (`set x` followed by `constant x`,
                // or a `to` after a `set`). Member names never reach this loop:
                // they are repaired once both member tables are complete below.
                continue;
            }
            let mut scope = Some(token_scope[index]);
            while let Some(scope_id) = scope {
                if let Some(binding) = scope_decls
                    .get(scope_id)
                    .and_then(|scope| scope.get(name))
                    .copied()
                {
                    if is_builtin_name(name)
                        && !deferred_builtin_binds_to(&bindings[binding], &tokens[index])
                    {
                        break;
                    }
                    token_binding[index] = if is_call {
                        scope_function_binding(
                            name,
                            binding,
                            scope_id,
                            Location::of_name(&tokens[index], name),
                            &bindings,
                            &binding_scopes,
                        )
                        .or(Some(binding))
                    } else {
                        scope_plain_binding(name, binding, scope_id, &bindings, &binding_scopes)
                    };
                    break;
                }
                scope = scope_parents.get(scope_id).copied().flatten();
            }
        }

        // Attach each child to the binding its `extends` name resolved to, but
        // only when the parent was already declared at the child's `object`
        // token. The interpreter resolves the chain at declaration time against
        // the objects the run has declared so far (`declare_object` refuses
        // `Object 'x' extends 'y', which is not declared`), so the parent must
        // be visible in the scopes enclosing the child: a parent written below
        // its child is not an edge, and neither is one buried in a scope the
        // child cannot see, such as an uncalled function body or an untaken
        // `if`/`unless` branch — that object never reaches `context.objects`
        // before the child's statement runs. An unknown parent has no entry and
        // member lookup stops there.
        for index in 0..tokens.len() {
            if !matches!(
                tokens.get(index).map(|token| &token.kind),
                Some(TokenKind::Object)
            ) || !matches!(
                tokens.get(index + 2).map(|token| &token.kind),
                Some(TokenKind::Extends)
            ) {
                continue;
            }
            let (Some(child), Some(parent)) = (
                token_binding.get(index + 1).copied().flatten(),
                token_binding.get(index + 3).copied().flatten(),
            ) else {
                continue;
            };
            if parent == child || bindings[parent].kind != DefinitionKind::Object {
                continue;
            }
            if (
                bindings[parent].location.line,
                bindings[parent].location.column,
            ) >= (
                bindings[child].location.line,
                bindings[child].location.column,
            ) {
                continue;
            }
            let visible = {
                let mut scope = Some(binding_scopes[child]);
                let mut found = false;
                while let Some(scope_id) = scope {
                    if binding_scopes[parent] == scope_id {
                        found = true;
                        break;
                    }
                    scope = scope_parents.get(scope_id).copied().flatten();
                }
                found
            };
            if !visible {
                continue;
            }
            object_parents.insert(child, parent);
        }

        // An object field used through a receiver declared below it could not
        // resolve during the walk, and a member read written before a later
        // field declaration could only see the same-named method. Now that
        // both member tables are complete, resolve every member access again
        // in source order so a chain like `a.b.c` resolves its receiver before
        // the access that uses it.
        let mut members: Vec<usize> = member_names.iter().copied().collect();
        members.sort_unstable();

        // A member write replaces the member's function value in the runtime
        // module table, so a later read through the module must not resolve the
        // stale published function. The runtime only performs the write if the
        // frame holding it actually runs, so each write carries its frame path
        // and applies only to reads in that frame or a frame nested inside it.
        // A write in an uncalled function therefore leaves the module table —
        // and every outside read — untouched.
        let module_member_writes: Vec<ModuleMemberWrite> = member_writes
            .iter()
            .filter_map(|(&index, path)| {
                let receiver = index.checked_sub(2)?;
                let name = name_of(&tokens[index])?.to_string();
                let receiver_binding = token_binding[receiver]?;
                let module = match bindings[receiver_binding].kind {
                    DefinitionKind::Module => receiver_binding,
                    DefinitionKind::Import => import_module_bindings
                        .get(&receiver_binding)
                        .copied()
                        .or_else(|| {
                            name_of(&tokens[receiver])
                                .and_then(|alias| module_aliases.get(alias))
                                .and_then(|module| module_name_bindings.get(module).copied())
                        })?,
                    _ => return None,
                };
                Some(ModuleMemberWrite {
                    index,
                    module,
                    name,
                    path: path.clone(),
                })
            })
            .collect();
        for index in members {
            let Some(receiver) = index.checked_sub(2) else {
                continue;
            };
            let Some(name) = name_of(&tokens[index]) else {
                continue;
            };
            let receiver_is_this = matches!(&tokens[receiver].kind, TokenKind::This);
            // A `this` receiver resolves through the object frame the method
            // body sits in, not through the token's own binding.
            let receiver_binding = if receiver_is_this {
                this_objects.get(&receiver).copied()
            } else {
                token_binding[receiver]
            };

            // A member of a program-declared module names the function the
            // module publishes, exactly as `Module.name()` does at runtime,
            // whether it is called or read as a function value. A standard
            // module has no declaration in this file, so it keeps the
            // unresolved answer the walk gave it.
            let module_receiver = if receiver_is_this {
                None
            } else {
                receiver_binding.and_then(|binding| match bindings[binding].kind {
                    DefinitionKind::Module => Some(binding),
                    DefinitionKind::Import => {
                        import_module_bindings.get(&binding).copied().or_else(|| {
                            name_of(&tokens[receiver])
                                .and_then(|alias| module_aliases.get(alias))
                                .and_then(|module| module_name_bindings.get(module).copied())
                        })
                    }
                    _ => None,
                })
            };
            if let Some(module) = module_receiver {
                let read_path = member_runtime_paths.get(&index);
                let replaced = read_path.is_some_and(|read_path| {
                    module_member_writes.iter().any(|write| {
                        write.module == module
                            && write.name == name
                            && write.index <= index
                            && runtime_path_encloses(&write.path, read_path)
                    })
                });
                if !replaced {
                    if let Some(member) = declared_module_functions
                        .get(&module)
                        .and_then(|members| members.get(name))
                    {
                        token_binding[index] = Some(*member);
                        continue;
                    }
                }
            }

            let Some(receiver_object) = receiver_binding else {
                continue;
            };
            // With both member tables and every parent edge complete the
            // answer is definitive, so it replaces the walk's provisional
            // one — including clearing it when a `this` read could land on
            // a descendant override.
            token_binding[index] = member_binding(
                &tokens,
                index,
                receiver_object,
                receiver_is_this,
                &MemberTables {
                    fields: &object_fields,
                    methods: &object_methods,
                    parents: &object_parents,
                },
                name,
            );
        }
    }

    if visible.is_none() {
        visible = Some(visible_names(&scopes));
    }

    Ok(Analysis {
        tokens,
        bindings,
        token_binding,
        member_names,
        type_names,
        syntax_names,
        visible: visible.unwrap_or_default(),
        object_fields,
        object_methods,
        object_parents,
        this_objects,
        module_aliases,
        import_module_bindings,
        declared_module_members,
        declared_module_members_by_binding,
    })
}

/// The mutable bookkeeping a declaration updates while scopes are walked.
struct DeclarationSink<'a> {
    token_binding: &'a mut [Option<usize>],
    declarations: &'a mut HashSet<usize>,
    type_names: &'a mut HashSet<usize>,
}

/// Declares every identifier in the parameter list starting at `start`, and
/// answers the index of the closing `)` (or `start` when there is no list).
fn declare_params(
    tokens: &[Token],
    start: usize,
    scopes: &mut [HashMap<String, usize>],
    bindings: &mut Vec<Binding>,
    binding_scopes: &mut Vec<usize>,
    scope_id: usize,
    sink: &mut DeclarationSink<'_>,
) -> usize {
    if !matches!(
        tokens.get(start).map(|token| &token.kind),
        Some(TokenKind::LeftParen)
    ) {
        return start;
    }

    let mut cursor = start + 1;
    while cursor < tokens.len() {
        match &tokens[cursor].kind {
            TokenKind::RightParen | TokenKind::Newline | TokenKind::Eof => return cursor,
            TokenKind::Comma => cursor += 1,
            TokenKind::Identifier(name) => {
                let location = Location::of_name(&tokens[cursor], name);
                let binding = declare(
                    scopes,
                    bindings,
                    binding_scopes,
                    scope_id,
                    name,
                    location,
                    DefinitionKind::Parameter,
                );
                mark_declaration(sink.token_binding, sink.declarations, cursor, binding);
                cursor += 1;

                let has_type = match tokens.get(cursor).map(|token| &token.kind) {
                    Some(TokenKind::Colon) => true,
                    Some(TokenKind::Identifier(word)) => word == "of",
                    _ => false,
                };
                if has_type {
                    cursor = skip_type_expression(tokens, cursor + 1, false, sink.type_names);
                }
            }
            _ => cursor += 1,
        }
    }

    cursor
}

/// The index of the identifier token covering a source offset.
fn name_token_at(analysis: &Analysis, map: &SourceMap, offset: usize) -> Option<usize> {
    analysis
        .tokens
        .iter()
        .enumerate()
        .find_map(|(index, token)| {
            let name = name_of(token)?;
            let start = map.offset(token.line, token.column);
            let width = name.chars().count();
            (offset >= start && offset < start + width).then_some(index)
        })
}

/// Goto-definition: the binding a name at `line:column` declares or uses.
///
/// A name that resolves to a builtin answers [`DefinitionKind::Builtin`] with
/// no file span; a name with no binding at all answers `None`.
pub fn goto_definition(
    source: &str,
    line: usize,
    column: usize,
) -> Result<Option<Definition>, String> {
    let map = SourceMap::new(source);
    let offset = map.validate(line, column)?;
    let analysis = analyze(source, None)?;

    if inside_text(&analysis.tokens, &map, offset) {
        return goto_definition_in_interpolation(&map, &analysis.tokens, offset);
    }

    let Some(index) = name_token_at(&analysis, &map, offset) else {
        return Ok(None);
    };

    Ok(definition_at_token(&analysis, index))
}

fn definition_at_token(analysis: &Analysis, index: usize) -> Option<Definition> {
    if let Some(binding) = analysis.token_binding[index] {
        return Some(Definition::from_binding(&analysis.bindings[binding]));
    }

    let name = name_of(&analysis.tokens[index]).expect("a name token has a name");
    if is_builtin_name(name)
        && !analysis.member_names.contains(&index)
        && !analysis.type_names.contains(&index)
        && !analysis.syntax_names.contains(&index)
    {
        Some(Definition::builtin(name))
    } else {
        None
    }
}

struct InterpolationSource {
    source: String,
    map: SourceMap,
    offset: usize,
    original: Vec<usize>,
    /// The decoded character range in `source` that holds the interpolated
    /// expressions. Prefix and suffix text is present only to keep the
    /// enclosing scopes analysable.
    expressions: Range<usize>,
}

impl InterpolationSource {
    fn original_location(&self, original_map: &SourceMap, location: Location) -> Location {
        let start = self.map.offset(location.line, location.column);
        let end = self.map.offset(location.end_line, location.end_column);
        let original_start = self
            .original
            .get(start)
            .copied()
            .unwrap_or(original_map.chars.len());
        let original_end = self.original.get(end).copied().unwrap_or(original_start);
        let (line, column) = line_column(original_map, original_start);
        let (end_line, end_column) = line_column(original_map, original_end);
        Location {
            line,
            column,
            end_line,
            end_column,
        }
    }

    fn original_definition(&self, original_map: &SourceMap, definition: Definition) -> Definition {
        if definition.line == 0 {
            return definition;
        }
        let location = self.original_location(
            original_map,
            Location {
                line: definition.line,
                column: definition.column,
                end_line: definition.end_line,
                end_column: definition.end_column,
            },
        );
        Definition {
            name: definition.name,
            kind: definition.kind,
            line: location.line,
            column: location.column,
            end_line: location.end_line,
            end_column: location.end_column,
        }
    }
}

fn line_column(map: &SourceMap, offset: usize) -> (usize, usize) {
    let index = map
        .line_starts
        .partition_point(|&start| start <= offset)
        .saturating_sub(1);
    let start = map.line_starts[index];
    (index + 1, offset.saturating_sub(start) + 1)
}

fn interpolation_source_at(
    map: &SourceMap,
    tokens: &[Token],
    offset: usize,
) -> Option<InterpolationSource> {
    let original: Vec<usize> = (0..map.chars.len()).collect();
    interpolation_source_with(map, tokens, offset, &original)
}

/// The view of the interpolation holding `offset`, descending into a text
/// literal written inside the interpolated expression when the caret is in
/// that literal's own substitution.
///
/// `parent_original` maps each character of `map` back to the root source, so
/// a view built from another view still reports positions the editor wrote.
/// The parser treats the outermost valid pair of one literal as the
/// substitution and only then parses the expression, which may contain a text
/// literal with a substitution of its own; descending mirrors that nesting.
fn interpolation_source_with(
    map: &SourceMap,
    tokens: &[Token],
    offset: usize,
    parent_original: &[usize],
) -> Option<InterpolationSource> {
    let literal = text_literal_at(tokens, map, offset)?;
    let interpolation = interpolation_at(&literal, offset)?;
    let caret = literal.caret_index(offset);
    let inner_start = interpolation.open + 1;
    let mut view = interpolation_view(map, &literal, &interpolation, parent_original);
    view.offset = literal.start + (caret - inner_start);

    if inside_nested_text(&literal, &interpolation, offset) {
        let analysis = analyze(&view.source, None).ok()?;
        return interpolation_source_with(&view.map, &analysis.tokens, view.offset, &view.original);
    }

    Some(view)
}

/// A copy of `map`'s source with `literal` replaced by the decoded expression
/// of `interpolation`, recording where each copied character was written.
fn interpolation_view(
    map: &SourceMap,
    literal: &TextLiteral,
    interpolation: &Interpolation,
    parent_original: &[usize],
) -> InterpolationSource {
    let inner_start = interpolation.open + 1;
    let suffix_start = literal.end + 1;

    let mut chars = Vec::with_capacity(map.chars.len());
    let mut original = Vec::with_capacity(map.chars.len());
    chars.extend_from_slice(&map.chars[..literal.start]);
    original.extend_from_slice(&parent_original[..literal.start]);
    chars.extend_from_slice(&literal.chars[inner_start..interpolation.close]);
    original.extend(
        literal.sources[inner_start..interpolation.close]
            .iter()
            .map(|&source| parent_original[source]),
    );
    chars.extend_from_slice(&map.chars[suffix_start..]);
    original.extend_from_slice(&parent_original[suffix_start..]);

    let source: String = chars.into_iter().collect();
    InterpolationSource {
        map: SourceMap::new(&source),
        source,
        offset: literal.start,
        original,
        expressions: literal.start..literal.start + (interpolation.close - inner_start),
    }
}

/// The most expression characters one literal may have parsed while looking
/// for its interpolations.
///
/// Pairing itself is linear, but deciding whether a pair is an expression has
/// to lex and parse its contents. A literal whose braces nest without forming
/// expressions would otherwise ask for the same suffix over and over. The cap
/// still reaches an innermost substitution first, because candidates are tried
/// from the inside out.
const MAX_INTERPOLATION_CHECK_CHARS: usize = 1_000_000;

/// The most text literals with substitutions one find-references query will
/// re-analyze. Each literal needs one analysis to see the scopes around it.
const MAX_INTERPOLATION_ANALYSES: usize = 1024;

/// The matching `}` of every `{`, found in one stack pass.
///
/// An unmatched `{` has no entry. This is the same pairing the parser makes,
/// but done once for the whole literal instead of restarting a forward scan at
/// every open.
fn matching_closes(chars: &[char]) -> Vec<Option<usize>> {
    let mut closes = vec![None; chars.len()];
    let mut opens = Vec::new();

    for (index, ch) in chars.iter().enumerate() {
        match ch {
            '{' => opens.push(index),
            '}' => {
                if let Some(open) = opens.pop() {
                    closes[open] = Some(index);
                }
            }
            _ => {}
        }
    }

    closes
}

/// Which brace pairs in `literal` read as one complete expression.
///
/// The answer is indexed by the decoded position of the `{`. Candidates are
/// tested from the innermost pair outward: a deeply nested literal reaches its
/// real substitution even if the outer braces are only text and the work cap
/// stops before every enclosing pair has been tried.
fn interpolation_pairs(literal: &TextLiteral, check_budget: &mut usize) -> Vec<Option<usize>> {
    let chars = &literal.chars;
    let closes = matching_closes(chars);
    let mut valid = vec![None; chars.len()];

    for open in (0..chars.len()).rev() {
        if chars[open] != '{' {
            continue;
        }
        let Some(close) = closes[open] else {
            continue;
        };
        let cost = close - open + 1;
        if *check_budget < cost {
            continue;
        }
        *check_budget -= cost;

        let inner: String = chars[open + 1..close].iter().collect();
        if is_complete_expression(&inner) {
            valid[open] = Some(close);
        }
    }

    valid
}

/// Every top-level brace pair in `literal` that the parser reads as an
/// interpolation.
///
/// A valid outer pair hides the pairs written inside its expression, exactly as
/// the parser's `index = scan + 1` advance does. A text literal inside that
/// expression is found again when the view of the outer expression is walked.
fn interpolations_in(literal: &TextLiteral, check_budget: &mut usize) -> Vec<Interpolation> {
    let valid = interpolation_pairs(literal, check_budget);
    let mut found = Vec::new();
    let mut index = 0usize;

    while index < literal.chars.len() {
        if let Some(close) = valid[index] {
            found.push(Interpolation { open: index, close });
            index = close + 1;
        } else {
            index += 1;
        }
    }

    found
}

/// A copy of the source with `literal` replaced by every interpolated
/// expression it contains, separated by newlines.
///
/// `parent_original` maps each character of `map` back to the root source, so a
/// view can be built from another view without losing the positions an editor
/// needs. `expressions` records the inserted range so references found in the
/// unchanged prefix and suffix are not reported a second time.
fn interpolations_view(
    map: &SourceMap,
    literal: &TextLiteral,
    interpolations: &[Interpolation],
    parent_original: &[usize],
) -> InterpolationSource {
    let suffix_start = literal.end + 1;
    let inserted = interpolations
        .iter()
        .map(|interpolation| interpolation.close - interpolation.open - 1)
        .sum::<usize>()
        + interpolations.len().saturating_sub(1);

    let mut chars = Vec::with_capacity(map.chars.len() + inserted);
    let mut original = Vec::with_capacity(map.chars.len() + inserted);
    chars.extend_from_slice(&map.chars[..literal.start]);
    original.extend_from_slice(&parent_original[..literal.start]);

    let expressions_start = chars.len();
    for (position, interpolation) in interpolations.iter().enumerate() {
        if position > 0 {
            chars.push('\n');
            original.push(parent_original[literal.start]);
        }
        let inner_start = interpolation.open + 1;
        let inner_end = interpolation.close;
        chars.extend_from_slice(&literal.chars[inner_start..inner_end]);
        original.extend(
            literal.sources[inner_start..inner_end]
                .iter()
                .map(|&source| parent_original[source]),
        );
    }
    let expressions_end = chars.len();

    chars.extend_from_slice(&map.chars[suffix_start..]);
    original.extend_from_slice(&parent_original[suffix_start..]);

    let source: String = chars.into_iter().collect();
    InterpolationSource {
        map: SourceMap::new(&source),
        source,
        offset: literal.start,
        original,
        expressions: expressions_start..expressions_end,
    }
}

/// Whether a literal's decoded text can contain `name` at all.
fn literal_contains_name(literal: &TextLiteral, name: &str) -> bool {
    let needle: Vec<char> = name.chars().collect();
    if needle.is_empty() {
        return true;
    }
    literal
        .chars
        .windows(needle.len())
        .any(|window| window == needle.as_slice())
}

fn goto_definition_in_interpolation(
    map: &SourceMap,
    tokens: &[Token],
    offset: usize,
) -> Result<Option<Definition>, String> {
    let Some(view) = interpolation_source_at(map, tokens, offset) else {
        return Ok(None);
    };
    let Ok(analysis) = analyze(&view.source, None) else {
        return Ok(None);
    };
    let Some(index) = name_token_at(&analysis, &view.map, view.offset) else {
        return Ok(None);
    };
    Ok(definition_at_token(&analysis, index)
        .map(|definition| view.original_definition(map, definition)))
}

/// Goto-definition as a JSON object, or `null` when nothing is named there.
pub fn goto_definition_json(source: &str, line: usize, column: usize) -> Result<String, String> {
    Ok(match goto_definition(source, line, column)? {
        Some(definition) => definition.to_json(),
        None => "null".to_string(),
    })
}

/// Find-references: every token that resolves to the same binding as the name at
/// `line:column`, in source order.
///
/// A declaration is one of its own references. Shadowed bindings of the same
/// name are different bindings, so an inner `x` never appears in an outer `x`'s
/// answer. A builtin has no declaration in the file, so its answer is every
/// unresolved use of that builtin name.
pub fn find_references(source: &str, line: usize, column: usize) -> Result<Vec<Location>, String> {
    let map = SourceMap::new(source);
    let offset = map.validate(line, column)?;
    let analysis = analyze(source, None)?;

    if inside_text(&analysis.tokens, &map, offset) {
        return find_references_in_interpolation(&map, &analysis, offset);
    }

    let Some(index) = name_token_at(&analysis, &map, offset) else {
        return Ok(Vec::new());
    };
    Ok(references_for(&map, &analysis, index))
}

fn references_for(map: &SourceMap, analysis: &Analysis, index: usize) -> Vec<Location> {
    let target = analysis.token_binding[index];
    let name = name_of(&analysis.tokens[index]).expect("a name token has a name");

    if target.is_none()
        && (analysis.member_names.contains(&index)
            || analysis.type_names.contains(&index)
            || analysis.syntax_names.contains(&index)
            || !is_builtin_name(name))
    {
        // A word with no binding is not a reference target. Only a builtin
        // resolves from the registry rather than from a declaration in the
        // file, so only a builtin name can have file references without a
        // binding.
        return Vec::new();
    }

    let declaration = target.map(|binding| analysis.bindings[binding].location);
    references_globally(map, analysis, declaration, name)
}

/// Every reference to a name, including uses written in any text literal's
/// `{…}` interpolation in the file.
///
/// The main token stream holds a whole text literal as one token, so an
/// interpolated use is invisible to [`references_matching`]. A literal with
/// substitutions is re-analyzed once with all of its expressions in place of
/// the literal, and a name that resolves to the same declaration is mapped back
/// onto the source. This keeps a declaration-side query and a query made inside
/// the interpolation answering the same set without re-analyzing the file once
/// per pair.
fn references_globally(
    map: &SourceMap,
    analysis: &Analysis,
    declaration: Option<Location>,
    name: &str,
) -> Vec<Location> {
    let binding = match declaration {
        Some(declaration) => match binding_declared_at(analysis, declaration) {
            Some(binding) => Some(binding),
            None => return Vec::new(),
        },
        None => None,
    };
    let mut found = references_matching(analysis, binding, name);
    let original: Vec<usize> = (0..map.chars.len()).collect();
    let mut analyses_left = MAX_INTERPOLATION_ANALYSES;
    let query = InterpolationQuery {
        root_map: map,
        declaration,
        name,
    };

    literal_references(
        &query,
        map,
        &original,
        analysis,
        None,
        &mut found,
        &mut analyses_left,
    );

    found.sort_by_key(|location| (location.line, location.column));
    found.dedup();
    found
}

struct InterpolationQuery<'a> {
    root_map: &'a SourceMap,
    declaration: Option<Location>,
    name: &'a str,
}

/// Adds the references written in text literals of one analysis.
///
/// `only` restricts the walk to text that was inserted for an outer
/// interpolation, so the unchanged prefix and suffix of a view are not visited
/// again. The recursion handles a text literal written inside an interpolated
/// expression, whose own substitutions are only visible after the outer view
/// has been analyzed.
fn literal_references(
    query: &InterpolationQuery,
    map: &SourceMap,
    parent_original: &[usize],
    analysis: &Analysis,
    only: Option<Range<usize>>,
    found: &mut Vec<Location>,
    analyses_left: &mut usize,
) {
    for token in &analysis.tokens {
        let Some(literal) = text_literal_token(map, token) else {
            continue;
        };
        if only
            .as_ref()
            .is_some_and(|range| !range.contains(&literal.start))
        {
            continue;
        }
        if *analyses_left == 0 {
            break;
        }
        if !literal_contains_name(&literal, query.name) {
            continue;
        }

        let mut check_budget = MAX_INTERPOLATION_CHECK_CHARS;
        let interpolations = interpolations_in(&literal, &mut check_budget);
        if interpolations.is_empty() {
            continue;
        }
        *analyses_left -= 1;

        let view = interpolations_view(map, &literal, &interpolations, parent_original);
        let Ok(interpolated) = analyze(&view.source, None) else {
            continue;
        };
        let binding = match query.declaration {
            Some(declaration) => {
                match binding_declared_in(&view, query.root_map, &interpolated, declaration) {
                    Some(binding) => Some(binding),
                    None => continue,
                }
            }
            None => None,
        };

        for location in references_matching(&interpolated, binding, query.name) {
            let start = view.map.offset(location.line, location.column);
            if view.expressions.contains(&start) {
                found.push(view.original_location(query.root_map, location));
            }
        }

        literal_references(
            query,
            &view.map,
            &view.original,
            &interpolated,
            Some(view.expressions.clone()),
            found,
            analyses_left,
        );
    }
}

fn references_matching(analysis: &Analysis, target: Option<usize>, name: &str) -> Vec<Location> {
    let mut found = Vec::new();
    for (candidate, token) in analysis.tokens.iter().enumerate() {
        if name_of(token) != Some(name) {
            continue;
        }
        let matches = match target {
            Some(binding) => analysis.token_binding[candidate] == Some(binding),
            None => {
                analysis.token_binding[candidate].is_none()
                    && !analysis.member_names.contains(&candidate)
                    && !analysis.type_names.contains(&candidate)
                    && !analysis.syntax_names.contains(&candidate)
            }
        };
        if matches {
            found.push(Location::of_name(token, name));
        }
    }

    found.sort_by_key(|location| (location.line, location.column));
    found
}

/// The binding declared at `declaration`, matched by the start of its name
/// because a view mapped back onto the source can round its end span.
fn binding_declared_at(analysis: &Analysis, declaration: Location) -> Option<usize> {
    analysis.bindings.iter().position(|binding| {
        binding.location.line == declaration.line && binding.location.column == declaration.column
    })
}

/// The binding in a re-analyzed interpolation whose declaration maps back to
/// the same source position as `declaration`.
fn binding_declared_in(
    view: &InterpolationSource,
    original_map: &SourceMap,
    analysis: &Analysis,
    declaration: Location,
) -> Option<usize> {
    let target = original_map.offset(declaration.line, declaration.column);
    analysis.bindings.iter().position(|binding| {
        let start = view
            .map
            .offset(binding.location.line, binding.location.column);
        view.original.get(start).copied() == Some(target)
    })
}

fn find_references_in_interpolation(
    map: &SourceMap,
    original: &Analysis,
    offset: usize,
) -> Result<Vec<Location>, String> {
    let Some(view) = interpolation_source_at(map, &original.tokens, offset) else {
        return Ok(Vec::new());
    };
    let Ok(analysis) = analyze(&view.source, None) else {
        return Ok(Vec::new());
    };
    let Some(index) = name_token_at(&analysis, &view.map, view.offset) else {
        return Ok(Vec::new());
    };

    let target = analysis.token_binding[index];
    let name = name_of(&analysis.tokens[index]).expect("a name token has a name");
    if target.is_none()
        && (analysis.member_names.contains(&index)
            || analysis.type_names.contains(&index)
            || analysis.syntax_names.contains(&index)
            || !is_builtin_name(name))
    {
        return Ok(Vec::new());
    }

    let Some(literal) = text_literal_at(&original.tokens, map, offset) else {
        return Ok(Vec::new());
    };
    let declaration =
        target.map(|binding| view.original_location(map, analysis.bindings[binding].location));
    let declared_inside_text = declaration.is_some_and(|declaration| {
        let start = map.offset(declaration.line, declaration.column);
        start >= literal.start && start <= literal.end
    });

    if declared_inside_text {
        // The declaration itself lives in this literal, so the main analysis
        // cannot see it. Answer from the re-analysis alone, mapped back.
        let mut found: Vec<Location> = references_matching(&analysis, target, name)
            .into_iter()
            .map(|location| view.original_location(map, location))
            .collect();
        found.sort_by_key(|location| (location.line, location.column));
        found.dedup();
        return Ok(found);
    }

    Ok(references_globally(map, original, declaration, name))
}

/// Find-references as the JSON array those locations render to.
pub fn find_references_json(source: &str, line: usize, column: usize) -> Result<String, String> {
    let locations = find_references(source, line, column)?;
    let entries: Vec<String> = locations
        .iter()
        .map(|location| location.to_json())
        .collect();
    Ok(format!("[{}]", entries.join(",")))
}

/// Completion: keywords, builtins/modules and every name in scope at
/// `line:column`, sorted and deduplicated.
///
/// Inside a string literal no code can be written, so the answer is empty
/// rather than a list of words that would all be text. The exception is a
/// `{…}` interpolation, whose contents the parser reads as an expression: there
/// the answer is that expression's names, or a receiver's members. A nested
/// text literal inside such an expression is text again.
pub fn completions(source: &str, line: usize, column: usize) -> Result<Vec<String>, String> {
    let map = SourceMap::new(source);
    let offset = map.validate(line, column)?;
    let analysis = analyze(source, Some(offset))?;

    if inside_text(&analysis.tokens, &map, offset) || inside_comment(&map, offset) {
        if let Some(literal) = text_literal_at(&analysis.tokens, &map, offset) {
            if let Some(interpolation) = interpolation_at(&literal, offset) {
                if inside_nested_text(&literal, &interpolation, offset) {
                    // The caret is inside a text literal written in the
                    // interpolated expression. If that literal has a
                    // substitution of its own, complete the deepest expression
                    // while keeping the scopes the outer expression opened.
                    let Some(view) = interpolation_source_at(&map, &analysis.tokens, offset) else {
                        return Ok(Vec::new());
                    };
                    let Ok(nested) = analyze(&view.source, None) else {
                        return Ok(Vec::new());
                    };
                    if member_slot(&nested, &view.map, view.offset).is_some() {
                        return Ok(
                            member_completions(&nested, &view.map, view.offset).unwrap_or_default()
                        );
                    }
                    return Ok(interpolation_completions(&nested));
                }
                if let Some(members) =
                    interpolation_member_completions(&map, &literal, &interpolation, offset)
                {
                    return Ok(members);
                }
                return Ok(interpolation_completions(&analysis));
            }
            // A `receiver.` member slot being typed inside a balanced pair is
            // still completion even when the expression after the dot is not
            // written yet. The parser would keep the pair as text until the
            // expression is complete; the editor can already name its members.
            if let Some(interpolation) = interpolation_candidate(&literal, offset) {
                if inside_nested_text(&literal, &interpolation, offset) {
                    return Ok(Vec::new());
                }
                if let Some(members) =
                    interpolation_member_completions(&map, &literal, &interpolation, offset)
                {
                    return Ok(members);
                }
            }
        }
        return Ok(Vec::new());
    }

    if member_slot(&analysis, &map, offset).is_some() {
        // A member list is order-independent, so answer from a complete walk
        // rather than the prefix of the file that is in lexical scope. The
        // prefix walk stops before the caret, so it cannot bind a receiver
        // declared below it; the complete walk can.
        let full = analyze(source, None)?;
        return Ok(member_completions(&full, &map, offset).unwrap_or_default());
    }

    let mut names: Vec<String> = analysis.visible;
    names.extend(KEYWORDS.iter().map(|(word, _)| (*word).to_string()));
    names.extend(BUILTIN_NAMES.iter().map(|name| (*name).to_string()));
    names.extend(MODULES.iter().map(|name| (*name).to_string()));
    names.push("PI".to_string());
    names.push("E".to_string());

    names.sort();
    names.dedup();
    Ok(names)
}

/// The syntactic `receiver.` slot at `offset`, before resolving the receiver.
struct MemberSlot {
    receiver: usize,
    prefix: String,
}

/// Whether `offset` sits in a `receiver.member` slot, and the partial member
/// already written after the dot.
///
/// The answer depends only on the tokens and the source text, not on whether
/// the receiver has a binding yet: completion must recognise a member slot
/// above an object declared later in the file.
fn member_slot(analysis: &Analysis, map: &SourceMap, offset: usize) -> Option<MemberSlot> {
    let dot = analysis.tokens.iter().rposition(|token| {
        matches!(token.kind, TokenKind::Dot) && map.offset(token.line, token.column) < offset
    })?;
    let dot_start = map.offset(analysis.tokens[dot].line, analysis.tokens[dot].column);
    let after_dot = dot_start + 1;
    if offset < after_dot || offset > map.chars.len() {
        return None;
    }

    let prefix: String = map.chars[after_dot..offset]
        .iter()
        .take_while(|c| c.is_alphanumeric() || **c == '_')
        .collect();
    if prefix.chars().count() != offset - after_dot {
        // Whitespace or another dot sits between the receiver and the caret:
        // the caret is not naming a member.
        return None;
    }

    let receiver = (0..dot)
        .rev()
        .find(|&index| !matches!(analysis.tokens[index].kind, TokenKind::Newline))?;
    Some(MemberSlot { receiver, prefix })
}

/// Completion for the `receiver.member` slot at `offset`.
///
/// An object offers its own fields and methods, including the ones it inherits
/// through `extends`; a module the program declared offers the names its
/// `export` publishes; a standard module offers the members in
/// [`MODULE_MEMBERS`]. A member name already written is a prefix, so only the
/// names that can still continue it are offered. `None` means the receiver is
/// not an object or a module namespace, so there is nothing member-specific to
/// offer.
fn member_completions(analysis: &Analysis, map: &SourceMap, offset: usize) -> Option<Vec<String>> {
    let MemberSlot { receiver, prefix } = member_slot(analysis, map, offset)?;
    let receiver_token = &analysis.tokens[receiver];
    let receiver_object = if matches!(receiver_token.kind, TokenKind::This) {
        analysis.this_objects.get(&receiver).copied()
    } else {
        analysis.token_binding[receiver]
            .filter(|binding| analysis.bindings[*binding].kind == DefinitionKind::Object)
    };

    if let Some(object) = receiver_object {
        let mut names = Vec::new();
        let mut current = Some(object);
        let mut seen = HashSet::new();
        while let Some(object) = current {
            if !seen.insert(object) {
                break;
            }
            if let Some(fields) = analysis.object_fields.get(&object) {
                names.extend(fields.keys().cloned());
            }
            if let Some(methods) = analysis.object_methods.get(&object) {
                names.extend(methods.keys().cloned());
            }
            current = analysis.object_parents.get(&object).copied();
        }
        names.sort();
        names.dedup();
        names.retain(|name| name.starts_with(&prefix));
        return Some(names);
    }

    let name = name_of(receiver_token)?;
    let receiver_binding = analysis.token_binding[receiver];
    let binding_kind = receiver_binding.map(|binding| analysis.bindings[binding].kind);

    // The alias table is flat, so only a binding that is actually in scope may
    // make an alias resolve. An import written inside a function or a branch
    // leaves its alias behind after `end`; that out-of-scope name has no
    // binding, so it must not reach the alias table. An unbound name is still a
    // standard module name, because the runtime lets a program write
    // `files.read(..)` without importing `files`.
    let (module_binding, module, namespace) = match binding_kind {
        Some(DefinitionKind::Import) => {
            let module_binding = receiver_binding
                .and_then(|binding| analysis.import_module_bindings.get(&binding).copied());
            let module = analysis
                .module_aliases
                .get(name)
                .map(String::as_str)
                .unwrap_or(name);
            let known = module_binding.is_some()
                || is_module(module)
                || analysis.declared_module_members.contains_key(module);
            (module_binding, module, known)
        }
        Some(DefinitionKind::Module) => {
            let known = receiver_binding.is_some_and(|binding| {
                analysis
                    .declared_module_members_by_binding
                    .contains_key(&binding)
            }) || is_module(name);
            (receiver_binding, name, known)
        }
        None => (None, name, is_module(name)),
        _ => return None,
    };
    if !namespace {
        return None;
    }

    let members: Vec<&str> = if let Some(declared) = module_binding
        .and_then(|binding| analysis.declared_module_members_by_binding.get(&binding))
        .or_else(|| analysis.declared_module_members.get(module))
    {
        declared.iter().map(String::as_str).collect()
    } else {
        MODULE_MEMBERS
            .iter()
            .find(|(listed, _)| *listed == module)
            .map(|(_, members)| *members)
            .unwrap_or(&[])
            .to_vec()
    };
    let mut names: Vec<String> = members
        .into_iter()
        .filter(|member| member.starts_with(&prefix))
        .map(str::to_string)
        .collect();
    names.sort();
    names.dedup();
    Some(names)
}

/// Completion as a JSON object carrying the requested position and the items.
pub fn completions_json(source: &str, line: usize, column: usize) -> Result<String, String> {
    SourceMap::new(source).validate(line, column)?;
    let items = completions(source, line, column)?;
    let rendered: Vec<String> = items.iter().map(|item| escape_json(item)).collect();

    Ok(format!(
        "{{\"line\":{},\"column\":{},\"items\":[{}]}}",
        line,
        column,
        rendered.join(","),
    ))
}

/// A text literal's decoded contents and the source offset each character came
/// from. The parser decides interpolation on the decoded text, so tooling has
/// to see `\u007B` as `{` too.
struct TextLiteral {
    /// The source offset of the opening quote.
    start: usize,
    /// The source offset of the closing quote.
    end: usize,
    /// The literal's text with every escape decoded.
    chars: Vec<char>,
    /// The source offset of each decoded character's first character.
    sources: Vec<usize>,
}

impl TextLiteral {
    /// How many decoded characters precede `offset`.
    fn caret_index(&self, offset: usize) -> usize {
        self.sources
            .iter()
            .take_while(|&&source| source < offset)
            .count()
    }
}

/// The text literal holding `offset`, decoded the way the lexer reads it.
fn text_literal_at(tokens: &[Token], map: &SourceMap, offset: usize) -> Option<TextLiteral> {
    tokens.iter().find_map(|token| {
        let literal = text_literal_token(map, token)?;
        (offset >= literal.start && offset <= literal.end).then_some(literal)
    })
}

/// The text literal a token holds, decoded the way the lexer reads it.
fn text_literal_token(map: &SourceMap, token: &Token) -> Option<TextLiteral> {
    if !matches!(token.kind, TokenKind::Text(_)) {
        return None;
    }
    let start = map.offset(token.line, token.column);
    decode_text_literal(map, start)
}

/// Decodes the text literal whose opening quote is at `start`, recording where
/// each decoded character was written so the caret can be mapped back to it.
fn decode_text_literal(map: &SourceMap, start: usize) -> Option<TextLiteral> {
    let chars = &map.chars;
    let mut decoded = Vec::new();
    let mut sources = Vec::new();
    let mut cursor = start + 1;

    while cursor < chars.len() {
        match chars[cursor] {
            '"' => {
                return Some(TextLiteral {
                    start,
                    end: cursor,
                    chars: decoded,
                    sources,
                });
            }
            '\\' => {
                let escape = cursor;
                cursor += 1;
                let escaped = *chars.get(cursor)?;
                let decoded_char = match escaped {
                    'n' => {
                        cursor += 1;
                        '\n'
                    }
                    't' => {
                        cursor += 1;
                        '\t'
                    }
                    'r' => {
                        cursor += 1;
                        '\r'
                    }
                    '\\' => {
                        cursor += 1;
                        '\\'
                    }
                    '"' => {
                        cursor += 1;
                        '"'
                    }
                    'u' => {
                        let mut value = 0u32;
                        let mut index = cursor + 1;
                        for _ in 0..4 {
                            value = value * 16 + chars.get(index)?.to_digit(16)?;
                            index += 1;
                        }
                        cursor = index;
                        char::from_u32(value)?
                    }
                    _ => return None,
                };
                decoded.push(decoded_char);
                sources.push(escape);
            }
            c => {
                decoded.push(c);
                sources.push(cursor);
                cursor += 1;
            }
        }
    }

    None
}

/// The brace pair the parser treats as an interpolation at `offset`.
#[derive(Clone, Copy)]
struct Interpolation {
    /// The decoded index of the opening `{`.
    open: usize,
    /// The decoded index of the matching `}`.
    close: usize,
}

/// The innermost balanced pair at `offset` that could still become an
/// interpolation once the member name being typed after its dot is finished.
///
/// A balanced pair alone is not enough: the parser keeps a pair as text unless
/// its contents read as one whole expression. Treating the missing member name
/// as written is what lets `{this.}` offer members before the name exists;
/// anything else, such as a JSON document with a receiver-shaped tail, stays
/// text.
fn interpolation_candidate(literal: &TextLiteral, offset: usize) -> Option<Interpolation> {
    let caret = literal.caret_index(offset);
    let closes = matching_closes(&literal.chars);
    let mut check_budget = MAX_INTERPOLATION_CHECK_CHARS;

    for open in (0..closes.len()).rev() {
        let Some(close) = closes[open] else {
            continue;
        };
        if !(open < caret && caret <= close) {
            continue;
        }
        let cost = close - open + 1;
        if check_budget < cost {
            continue;
        }
        check_budget -= cost;

        let interpolation = Interpolation { open, close };
        if could_become_interpolation(literal, &interpolation) {
            return Some(interpolation);
        }
    }

    None
}

/// Whether the pair could be a substitution once a member name that is still
/// being typed is treated as complete.
fn could_become_interpolation(literal: &TextLiteral, interpolation: &Interpolation) -> bool {
    let inner: String = literal.chars[interpolation.open + 1..interpolation.close]
        .iter()
        .collect();
    if is_complete_expression(&inner) {
        return true;
    }
    let trimmed = inner.trim_end();
    if !trimmed.ends_with('.') {
        return false;
    }
    is_complete_expression(&format!("{trimmed}placeholder"))
}

/// Whether `offset` falls inside a quoted text literal written in the decoded
/// expression of `interpolation`.
///
/// The outer literal is already decoded, so the quotes that delimit a nested
/// literal appear as ordinary `"` characters here. A caret at or between them
/// is inside text, not code, even though the enclosing interpolation is a valid
/// expression.
fn inside_nested_text(literal: &TextLiteral, interpolation: &Interpolation, offset: usize) -> bool {
    let caret = literal.caret_index(offset);
    let mut quoted = false;
    let mut escaped = false;
    let mut index = interpolation.open + 1;

    while index < interpolation.close {
        let ch = literal.chars[index];
        if index == caret {
            return quoted || ch == '"';
        }
        if escaped {
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == '"' {
            quoted = !quoted;
        }
        index += 1;
    }

    false
}

/// The outermost brace pair at `offset` whose contents re-parse as one complete
/// expression, which is the parser's own rule for interpolation.
///
/// The parser walks a literal from left to right and, when a pair is valid,
/// advances past the whole span instead of reading the pairs inside it. A
/// nested pair therefore belongs to the outer expression rather than becoming
/// a second substitution. A balanced pair holding a JSON document,
/// `{ "name": "Alice" }`, is text to the parser and stays text here.
fn interpolation_at(literal: &TextLiteral, offset: usize) -> Option<Interpolation> {
    let caret = literal.caret_index(offset);
    let mut check_budget = MAX_INTERPOLATION_CHECK_CHARS;
    let valid = interpolation_pairs(literal, &mut check_budget);

    for (open, close) in valid.into_iter().enumerate() {
        let Some(close) = close else {
            continue;
        };
        if open < caret && caret <= close {
            return Some(Interpolation { open, close });
        }
    }

    None
}

/// Completion for a `receiver.member` slot written inside an interpolation.
///
/// The main token stream holds the whole literal as one text token, so the
/// receiver cannot be resolved there. Re-analyze a copy of the source with the
/// literal replaced by the interpolation's decoded expression; that keeps the
/// enclosing scopes, objects and modules, and puts the receiver's tokens where
/// [`member_completions`] can resolve them.
fn interpolation_member_completions(
    map: &SourceMap,
    literal: &TextLiteral,
    interpolation: &Interpolation,
    offset: usize,
) -> Option<Vec<String>> {
    let caret = literal.caret_index(offset);
    let inner_start = interpolation.open + 1;
    if caret < inner_start || caret > interpolation.close {
        return None;
    }

    let mut synthetic: Vec<char> = Vec::with_capacity(map.chars.len());
    synthetic.extend_from_slice(&map.chars[..literal.start]);
    synthetic.extend_from_slice(&literal.chars[inner_start..interpolation.close]);
    synthetic.extend_from_slice(&map.chars[literal.end + 1..]);
    let source: String = synthetic.into_iter().collect();
    let synthetic_offset = literal.start + (caret - inner_start);
    let synthetic_map = SourceMap::new(&source);
    let full = analyze(&source, None).ok()?;
    member_slot(&full, &synthetic_map, synthetic_offset)?;
    Some(member_completions(&full, &synthetic_map, synthetic_offset).unwrap_or_default())
}

/// Completion for the expression written inside a text interpolation.
///
/// Only what an expression can use is offered: the names in scope, builtins,
/// modules and constants, and the operator/literal keywords. A statement
/// keyword such as `say`, `set` or `end` cannot start an interpolated
/// expression, so it is not offered in a position that is code inside text.
/// `this` is a keyword rather than a lexical name, but a method may write it
/// in an expression, so it is offered too.
fn interpolation_completions(analysis: &Analysis) -> Vec<String> {
    const EXPRESSION_KEYWORDS: &[&str] = &[
        "and", "is", "mod", "no", "not", "nothing", "or", "this", "yes",
    ];

    let mut names = analysis.visible.clone();
    names.extend(EXPRESSION_KEYWORDS.iter().map(|word| (*word).to_string()));
    names.extend(BUILTIN_NAMES.iter().map(|name| (*name).to_string()));
    names.extend(MODULES.iter().map(|name| (*name).to_string()));
    names.push("PI".to_string());
    names.push("E".to_string());
    names.sort();
    names.dedup();
    names
}

/// Whether `offset` falls inside a string literal's source text.
fn inside_text(tokens: &[Token], map: &SourceMap, offset: usize) -> bool {
    tokens.iter().any(|token| {
        if !matches!(token.kind, TokenKind::Text(_)) {
            return false;
        }
        let start = map.offset(token.line, token.column);
        let mut cursor = start + 1;
        while cursor < map.chars.len() {
            match map.chars[cursor] {
                '\\' => cursor += 2,
                '"' => return offset >= start && offset <= cursor,
                _ => cursor += 1,
            }
        }
        offset >= start
    })
}

/// Whether `offset` falls inside a `//` comment's source text.
///
/// The lexer drops comments before they become tokens, so the scan runs over the
/// source characters. It tracks string literals (and their escapes) so a `//`
/// inside text is not mistaken for a comment, and comments never cross a
/// newline, so the first line break ends one.
fn inside_comment(map: &SourceMap, offset: usize) -> bool {
    let chars = &map.chars;
    let mut index = 0;
    let mut in_text = false;

    while index < chars.len() && index <= offset {
        let c = chars[index];
        if in_text {
            if c == '\\' {
                index += 2;
                continue;
            }
            if c == '"' {
                in_text = false;
            }
            index += 1;
        } else if c == '"' {
            in_text = true;
            index += 1;
        } else if c == '/' && chars.get(index + 1) == Some(&'/') {
            let mut end = index;
            while end < chars.len() && chars[end] != '\n' && chars[end] != '\r' {
                end += 1;
            }
            if offset >= index && offset <= end {
                return true;
            }
            index = end.max(index + 1);
        } else {
            index += 1;
        }
    }

    false
}

/// The word-boundary alternation that matches every keyword, as a regex source
/// string: `\b(?:set|to|...)\b`.
pub fn keyword_pattern() -> String {
    format!(
        r"\b(?:{})\b",
        KEYWORDS
            .iter()
            .map(|(w, _)| *w)
            .collect::<Vec<_>>()
            .join("|")
    )
}

/// Escapes a string for use as a JSON string literal, including the quotes.
pub fn escape_json(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');

    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            // JSON requires the other C0 controls to be escaped; \u{XX} is the
            // only form it accepts for them.
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }

    out.push('"');
    out
}

/// The TextMate grammar for `.rb` files, as JSON text.
///
/// Rendered rather than hand-written so the keyword set comes straight from the
/// lexer. Write the result to [`GRAMMAR_PATH`] to publish it.
pub fn textmate_grammar() -> String {
    let mut patterns: Vec<String> = Vec::new();

    // Order matters to TextMate: of the patterns matching at the same
    // position, the first in this list wins.
    //
    // Strings come before comments so a `//` inside a string is string text,
    // not a comment: the string rule claims the quote and everything up to the
    // closing quote before the comment rule is ever offered the inner `//`.
    patterns.push(
        r#"{"name":"string.quoted.double.redblue","begin":"\"","end":"\"","patterns":[{"name":"constant.character.escape.redblue","match":"\\\\."}]}"#
            .to_string(),
    );
    // Comments come before the operator rule because the language's only
    // comment marker, `//`, starts with the division operator: without this
    // order a trailing comment highlights as an operator and as text. This is
    // the marker's the lexer skips (see `Lexer::skip_comment`) and the one
    // `docs/GRAMMAR.md` specifies — `#` is not a comment in Redblue, it is an
    // unexpected character.
    patterns.push(r##"{"name":"comment.line.double-slash.redblue","match":"//.*$"}"##.to_string());
    // `yes`, `no` and `nothing` are the language's constants. `true`/`false`
    // are not: they only appear inside JSON text, where the string rule owns
    // them, so highlighting them here would mark a variable named `true`.
    patterns.push(
        r#"{"name":"constant.language.redblue","match":"\\b(?:yes|no|nothing)\\b"}"#.to_string(),
    );
    patterns.push(
        r#"{"name":"constant.numeric.redblue","match":"(?:\\d+(?:\\.\\d+)?(?:[eE][+-]?\\d+)?|\\.\\d+)"}"#
            .to_string(),
    );
    patterns.push(format!(
        r#"{{"name":"keyword.control.redblue","match":{}}}"#,
        escape_json(&keyword_pattern())
    ));
    patterns.push(
        r#"{"name":"entity.name.function.redblue","match":"\\b[A-Za-z_][A-Za-z0-9_]*(?=\\s*\\()"}"#
            .to_string(),
    );
    patterns.push(
        r#"{"name":"keyword.operator.redblue","match":"==|!=|<=|>=|<|>|\\+|-|\\*|/|%"}"#
            .to_string(),
    );
    patterns
        .push(r#"{"name":"punctuation.separator.redblue","match":"[,.()\\[\\]{}]"}"#.to_string());

    format!(
        concat!(
            "{{\n",
            "  \"$schema\": \"https://raw.githubusercontent.com/martinring/tmlanguage/master/tmlanguage.json\",\n",
            "  \"name\": \"Redblue\",\n",
            "  \"scopeName\": \"source.redblue\",\n",
            "  \"fileTypes\": [\"rb\"],\n",
            "  \"patterns\": [\n    {}\n  ]\n",
            "}}\n"
        ),
        patterns.join(",\n    "),
    )
}
