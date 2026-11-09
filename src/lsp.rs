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

use crate::error::Error;
use crate::lexer::{Lexer, Token, TokenKind, KEYWORDS};
use crate::stdlib::{BUILTIN_NAMES, MODULES};

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
            closes_at_token: None,
        }
    }

    fn scoped(kind: FrameKind) -> Self {
        Self {
            kind,
            opens_scope: true,
            catch_scope: false,
            object: None,
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
    /// The names visible where analysis stopped, for completion.
    visible: Vec<String>,
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
    });
    binding_scopes.push(scope_id);
    scope.insert(name.to_string(), id);
    id
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
    }
}

/// Whether the name at `index` follows a `.` and therefore names a field of
/// its receiver rather than a lexical binding.
fn is_member_access(tokens: &[Token], index: usize) -> bool {
    index
        .checked_sub(1)
        .and_then(|dot| tokens.get(dot))
        .is_some_and(|token| matches!(token.kind, TokenKind::Dot))
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
    let mut member_names: HashSet<usize> = HashSet::new();
    let mut object_fields: HashMap<usize, HashMap<String, usize>> = HashMap::new();
    let mut this_objects: HashMap<usize, usize> = HashMap::new();
    let mut frames: Vec<Frame> = Vec::new();
    let mut visible = None;
    let mut last_significant_line: Option<usize> = None;
    let mut last_significant: Option<usize> = None;
    let mut index = 0;

    while index < tokens.len() {
        close_due_frames(&mut frames, &mut scopes, &mut scope_ids, index);
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
                    frames.push(Frame::block(FrameKind::If));
                }
            }
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
                if let Some(name) = tokens.get(index + 1).and_then(name_of) {
                    let location = Location::of_name(&tokens[index + 1], name);
                    let binding = declare(
                        &mut scopes,
                        &mut bindings,
                        &mut binding_scopes,
                        *scope_ids.last().expect("the root scope always exists"),
                        name,
                        location,
                        DefinitionKind::Module,
                    );
                    mark_declaration(&mut token_binding, &mut declarations, index + 1, binding);
                }
                frames.push(Frame::scoped(FrameKind::Module));
                push_scope(
                    &mut scopes,
                    &mut scope_ids,
                    &mut scope_parents,
                    &mut next_scope_id,
                );
            }
            TokenKind::Test => frames.push(Frame::block(FrameKind::Test)),
            TokenKind::Try => frames.push(Frame::block(FrameKind::Try)),
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
                    let binding = declare(
                        &mut scopes,
                        &mut bindings,
                        &mut binding_scopes,
                        *scope_ids.last().expect("the root scope always exists"),
                        &name,
                        location,
                        DefinitionKind::Function,
                    );
                    mark_declaration(&mut token_binding, &mut declarations, cursor, binding);
                    frames.push(Frame::scoped(FrameKind::To));
                    push_scope(
                        &mut scopes,
                        &mut scope_ids,
                        &mut scope_parents,
                        &mut next_scope_id,
                    );
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
                while cursor < tokens.len() {
                    match tokens[cursor].kind.clone() {
                        TokenKind::Newline | TokenKind::Eof => break,
                        TokenKind::Identifier(name) => {
                            let location = Location::of_name(&tokens[cursor], &name);
                            let binding = declare(
                                &mut scopes[..1],
                                &mut bindings,
                                &mut binding_scopes,
                                0,
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
                            cursor += 1;
                        }
                        TokenKind::As | TokenKind::To => {
                            cursor += 1;
                            if let Some(name) = tokens.get(cursor).and_then(name_of) {
                                let location = Location::of_name(&tokens[cursor], name);
                                let binding = declare(
                                    &mut scopes[..1],
                                    &mut bindings,
                                    &mut binding_scopes,
                                    0,
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
                    // `set` writes through to an enclosing binding when one
                    // exists (the interpreter's `set_var` does the same), so
                    // reuse it instead of declaring a fresh inner name. Only a
                    // name no scope binds is a new declaration.
                    let existing = scopes
                        .iter()
                        .rev()
                        .find_map(|scope| scope.get(name).copied());
                    if let Some(binding) = existing {
                        token_binding[cursor] = Some(binding);
                    } else {
                        let location = Location::of_name(&tokens[cursor], name);
                        let binding = declare(
                            &mut scopes,
                            &mut bindings,
                            &mut binding_scopes,
                            *scope_ids.last().expect("the root scope always exists"),
                            name,
                            location,
                            kind,
                        );
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
            TokenKind::Has => {
                let first_on_line = last_significant_line != Some(token.line);
                let in_object = matches!(
                    frames.last(),
                    Some(frame) if frame.kind == FrameKind::Object
                );
                if first_on_line && in_object {
                    if let Some(name) = tokens.get(index + 1).and_then(name_of) {
                        let location = Location::of_name(&tokens[index + 1], name);
                        let binding = declare(
                            &mut scopes,
                            &mut bindings,
                            &mut binding_scopes,
                            *scope_ids.last().expect("the root scope always exists"),
                            name,
                            location,
                            DefinitionKind::Variable,
                        );
                        mark_declaration(&mut token_binding, &mut declarations, index + 1, binding);
                        if let Some(owner) = frames.last().and_then(|frame| frame.object) {
                            object_fields
                                .entry(owner)
                                .or_default()
                                .insert(name.to_string(), binding);
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
                if type_names.contains(&index) {
                    // A type annotation names a type, not the variable of the
                    // same spelling that happens to be in scope.
                } else if is_member_access(&tokens, index) {
                    member_names.insert(index);
                    let receiver = index
                        .checked_sub(2)
                        .and_then(|receiver| tokens.get(receiver))
                        .and_then(|token| match token.kind {
                            TokenKind::This => frames.iter().rev().find_map(|frame| frame.object),
                            _ => token_binding[index - 2],
                        });
                    if let Some(binding) =
                        receiver.and_then(|receiver| object_fields.get(&receiver))
                    {
                        token_binding[index] = binding.get(name).copied();
                    }
                } else {
                    token_binding[index] = scopes
                        .iter()
                        .rev()
                        .find_map(|scope| scope.get(name).copied());
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
            scope_decls[*scope].insert(bindings[binding].name.clone(), binding);
        }

        for index in 0..tokens.len() {
            let Some(name) = name_of(&tokens[index]) else {
                continue;
            };
            if token_binding[index].is_some()
                || declarations.contains(&index)
                || type_names.contains(&index)
                || member_names.contains(&index)
            {
                continue;
            }

            let mut scope = Some(token_scope[index]);
            while let Some(scope_id) = scope {
                if let Some(binding) = scope_decls
                    .get(scope_id)
                    .and_then(|scope| scope.get(name))
                    .copied()
                {
                    token_binding[index] = Some(binding);
                    break;
                }
                scope = scope_parents.get(scope_id).copied().flatten();
            }
        }

        // An object field used through a receiver declared below it could not
        // resolve during the walk. Now that every receiver name has a binding,
        // resolve member accesses too, in source order so a chain like
        // `a.b.c` resolves its receiver before the access that uses it.
        let mut members: Vec<usize> = member_names.iter().copied().collect();
        members.sort_unstable();
        for index in members {
            if token_binding[index].is_some() {
                continue;
            }
            let Some(receiver) = index.checked_sub(2) else {
                continue;
            };
            let Some(name) = name_of(&tokens[index]) else {
                continue;
            };
            let receiver_binding = match &tokens[receiver].kind {
                TokenKind::This => this_objects.get(&receiver).copied(),
                _ => token_binding[receiver],
            };
            if let Some(field) = receiver_binding
                .and_then(|receiver| object_fields.get(&receiver))
                .and_then(|fields| fields.get(name))
            {
                token_binding[index] = Some(*field);
            }
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
        visible: visible.unwrap_or_default(),
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

    let Some(index) = name_token_at(&analysis, &map, offset) else {
        return Ok(None);
    };

    if let Some(binding) = analysis.token_binding[index] {
        return Ok(Some(Definition::from_binding(&analysis.bindings[binding])));
    }

    let name = name_of(&analysis.tokens[index]).expect("a name token has a name");
    if is_builtin_name(name) && !analysis.member_names.contains(&index) {
        Ok(Some(Definition::builtin(name)))
    } else {
        Ok(None)
    }
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

    let Some(index) = name_token_at(&analysis, &map, offset) else {
        return Ok(Vec::new());
    };
    let target = analysis.token_binding[index];
    let name = name_of(&analysis.tokens[index]).expect("a name token has a name");

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
            }
        };
        if matches {
            found.push(Location::of_name(token, name));
        }
    }

    found.sort_by_key(|location| (location.line, location.column));
    Ok(found)
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
/// rather than a list of words that would all be text.
pub fn completions(source: &str, line: usize, column: usize) -> Result<Vec<String>, String> {
    let map = SourceMap::new(source);
    let offset = map.validate(line, column)?;
    let analysis = analyze(source, Some(offset))?;

    if inside_text(&analysis.tokens, &map, offset) || inside_comment(&map, offset) {
        return Ok(Vec::new());
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
