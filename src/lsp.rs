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

use crate::error::Error;
use crate::lexer::KEYWORDS;

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
/// The compile stages stop at the first failure — there is no program to
/// analyze once lexing fails — so at most one error is reported, followed by
/// the linter's findings when the program does parse.
///
/// The source is lexed and parsed exactly once: the AST that clears the
/// analyzer is the one the linter reads ([`crate::linter::lint_program`]), so
/// `rb diagnostics` does not run the frontend twice per file. The findings are
/// in a fixed order for a given source — the linter sorts its unused-variable
/// warnings by name — so the JSON an editor diffs between saves does not churn.
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

    if let Err(error) = crate::analyzer::analyze(&ast) {
        found.push(Diagnostic::from_error(&error));
        return found;
    }

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
