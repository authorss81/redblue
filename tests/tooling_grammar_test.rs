//! Editor/LSP tooling: the shipped TextMate grammar must cover the keyword set
//! the lexer actually recognises, and diagnostics must surface the spanned
//! lexer/parser/analyzer errors `run_source` reports.

use std::fs;
use std::path::PathBuf;

use redblue::lexer::Lexer;
use redblue::lsp::{
    diagnostics, diagnostics_json, keyword_pattern, textmate_grammar, Diagnostic, Severity,
    GRAMMAR_PATH, LANGUAGE_CONFIG_PATH,
};
use redblue::run_source;

fn manifest_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

/// The single error diagnostic, or a failure naming what came back instead.
fn error_diagnostic(source: &str) -> Diagnostic {
    let all = diagnostics(source);
    let diagnostic = all
        .iter()
        .find(|d| d.severity == Severity::Error)
        .unwrap_or_else(|| panic!("expected an error diagnostic, got {:?}", all));

    diagnostic.clone()
}

// ---------------------------------------------------------------------------
// The grammar covers the keyword set
// ---------------------------------------------------------------------------

#[test]
fn grammar_covers_the_lexer_keyword_set() {
    let keywords = Lexer::keywords();
    assert!(
        !keywords.is_empty(),
        "the lexer must publish a non-empty keyword set for tooling to use"
    );

    let pattern = keyword_pattern();
    let alternation = pattern
        .strip_prefix(r"\b(?:")
        .and_then(|rest| rest.strip_suffix(r")\b"))
        .unwrap_or_else(|| panic!("keyword pattern must be a bounded alternation: {}", pattern));

    let words: Vec<&str> = alternation.split('|').collect();

    for keyword in keywords {
        assert!(
            words.contains(keyword),
            "the grammar keyword pattern omits `{}`",
            keyword
        );
        assert!(
            textmate_grammar().contains(keyword),
            "the generated TextMate grammar omits `{}`",
            keyword
        );
    }

    assert_eq!(
        words.len(),
        keywords.len(),
        "the grammar lists {} words but the lexer has {} keywords",
        words.len(),
        keywords.len()
    );
}

#[test]
fn every_listed_keyword_lexes_as_a_keyword_not_an_identifier() {
    for keyword in Lexer::keywords() {
        let tokens =
            Lexer::tokenize(keyword).unwrap_or_else(|e| panic!("`{}` should lex: {}", keyword, e));

        assert!(
            !tokens.is_empty() && tokens[0].kind != redblue::lexer::TokenKind::Eof,
            "`{}` produced no token",
            keyword
        );
        assert_ne!(
            tokens[0].kind,
            redblue::lexer::TokenKind::Identifier(keyword.to_string()),
            "`{}` lexed as a plain identifier, so the grammar would not match it",
            keyword
        );
    }
}

#[test]
fn shipped_grammar_file_matches_the_generator() {
    let path = manifest_path(GRAMMAR_PATH);
    let shipped = fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "{} must ship in the repo so no build step is needed: {}",
            GRAMMAR_PATH, e
        )
    });

    assert_eq!(
        shipped,
        textmate_grammar(),
        "{} is stale; regenerate it with `rb grammar > {}`",
        GRAMMAR_PATH,
        GRAMMAR_PATH
    );
}

#[test]
fn shipped_grammar_file_is_valid_json_with_a_redblue_scope() {
    let path = manifest_path(GRAMMAR_PATH);
    let shipped = fs::read_to_string(&path).expect("shipped grammar should be readable");

    let value: serde_json::Value = serde_json::from_str(&shipped)
        .unwrap_or_else(|e| panic!("grammar is not valid JSON: {}", e));

    assert_eq!(
        value["scopeName"].as_str(),
        Some("source.redblue"),
        "the grammar must declare the source.redblue scope"
    );
    assert!(
        value["patterns"].as_array().is_some_and(|p| !p.is_empty()),
        "the grammar must declare at least one top-level pattern"
    );
}

// ---------------------------------------------------------------------------
// The grammar's comment rule is the marker's the lexer actually skips
// ---------------------------------------------------------------------------

/// The top-level patterns of the generated grammar.
fn grammar_patterns() -> Vec<serde_json::Value> {
    let grammar = textmate_grammar();

    let parsed: serde_json::Value =
        serde_json::from_str(&grammar).expect("the generator must emit valid JSON");

    parsed["patterns"]
        .as_array()
        .expect("the grammar must declare a patterns array")
        .clone()
}

/// The one top-level pattern whose scope name starts with `prefix`.
fn pattern_scoped(patterns: &[serde_json::Value], prefix: &str) -> serde_json::Value {
    let matching: Vec<&serde_json::Value> = patterns
        .iter()
        .filter(|p| {
            p["name"]
                .as_str()
                .is_some_and(|name| name.starts_with(prefix))
        })
        .collect();

    assert_eq!(
        matching.len(),
        1,
        "expected exactly one `{}` rule, found {} in {:?}",
        prefix,
        matching.len(),
        patterns
    );

    matching[0].clone()
}

#[test]
fn grammar_highlights_the_comment_marker_the_lexer_accepts() {
    let patterns = grammar_patterns();
    let comment = pattern_scoped(&patterns, "comment.");

    assert_eq!(
        comment["match"].as_str(),
        Some("//.*$"),
        "Redblue comments start with `//` (docs/GRAMMAR.md, SPEC.md); a grammar \
         that dims any other marker makes the editor lie about the source"
    );

    // The pin that matters: the marker in the grammar must be the marker the
    // lexer skips, and `#` must not be one. `textmate_grammar()` is the only
    // generator of that file, so pinning it pins the shipped copy too — the
    // staleness test above fails if the two ever diverge.
    let slash_source = "set x to 1\n// a comment\nsay x\n";
    // The commented line lexes as the blank line it is: the lexer skips the
    // comment text but keeps the newline that ends it.
    let bare_source = "set x to 1\n\nsay x\n";
    let kinds = |source: &str| -> Vec<redblue::lexer::TokenKind> {
        Lexer::tokenize(source)
            .unwrap_or_else(|e| panic!("`{}` must lex: {}", source, e.message()))
            .into_iter()
            .map(|token| token.kind)
            .collect()
    };

    assert_eq!(
        kinds(slash_source),
        kinds(bare_source),
        "`//` must be skipped by the lexer, or the grammar highlights a comment \
         the compiler rejects"
    );

    let number_sign = Lexer::tokenize("set x to 1\n# not a comment in Redblue\n")
        .expect_err("`#` is not a comment in Redblue, so the grammar must never claim it is one");
    assert!(
        number_sign.message().contains("Unexpected character"),
        "the lexer must reject `#`, got: {}",
        number_sign.message()
    );

    assert!(
        !textmate_grammar().contains('#'),
        "no rule in the grammar may mention `#`: the character is not valid Redblue"
    );
}

#[test]
fn grammar_orders_strings_before_comments_and_comments_before_operators() {
    let patterns = grammar_patterns();
    let index = |prefix: &str| -> usize {
        patterns
            .iter()
            .position(|p| {
                p["name"]
                    .as_str()
                    .is_some_and(|name| name.starts_with(prefix))
            })
            .unwrap_or_else(|| panic!("the grammar must declare a `{}` rule", prefix))
    };

    // TextMate tries the rules in order wherever more than one matches, and a
    // comment marker can appear inside both of the neighbours: `say "a // b"`
    // must stay one string, and `set x to 1 // note` must not highlight as a
    // division operator.
    assert!(
        index("string.") < index("comment."),
        "the string rule must come first, or a `//` inside a string starts a comment"
    );
    assert!(
        index("comment.") < index("keyword.operator."),
        "the comment rule must come before the operator rule, or the `/` of a \
         trailing `//` comment highlights as division"
    );
}

#[test]
fn shipped_language_configuration_uses_the_lexers_comment_marker() {
    let path = manifest_path(LANGUAGE_CONFIG_PATH);
    let shipped = fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "{} must ship in the repo so the extension is loadable without a \
             build step: {}",
            LANGUAGE_CONFIG_PATH, e
        )
    });

    let value: serde_json::Value = serde_json::from_str(&shipped)
        .unwrap_or_else(|e| panic!("{} is not valid JSON: {}", LANGUAGE_CONFIG_PATH, e));

    let marker = value["comments"]["lineComment"]
        .as_str()
        .expect("the language configuration must declare a line comment marker");

    assert_eq!(
        marker, "//",
        "the editor must insert the marker the language accepts ({}), not one \
         `rb run` rejects",
        LANGUAGE_CONFIG_PATH
    );

    // The marker the editor inserts has to be one the compiler skips, or
    // `Ctrl+/` writes a file that does not run.
    let commented = format!("set x to 1\n{} a comment\nsay x\n", marker);
    let blanked = "set x to 1\n\nsay x\n";
    let kinds = |source: &str| -> Vec<redblue::lexer::TokenKind> {
        Lexer::tokenize(source)
            .unwrap_or_else(|e| panic!("`{}` must lex: {}", source, e.message()))
            .into_iter()
            .map(|token| token.kind)
            .collect()
    };

    assert_eq!(
        kinds(&commented),
        kinds(blanked),
        "a line the editor comments out must lex the same as the line blanked out"
    );
    assert!(
        !value.to_string().contains("\"#\""),
        "`#` is not a Redblue comment marker, so nothing in {} may name it",
        LANGUAGE_CONFIG_PATH
    );
}

// ---------------------------------------------------------------------------
// Diagnostics surface spanned errors
// ---------------------------------------------------------------------------

#[test]
fn diagnostics_surface_a_spanned_lexer_error() {
    let source = "say \"unterminated\n";

    let diagnostic = error_diagnostic(source);

    assert_eq!(diagnostic.severity, Severity::Error);
    assert_eq!(
        (diagnostic.line, diagnostic.column),
        (1, 5),
        "the diagnostic must carry the span the error reports"
    );
    assert!(
        diagnostic.message.contains("Unterminated string"),
        "the diagnostic must carry the error message, got: {}",
        diagnostic.message
    );

    let expected = run_source(source).expect_err("the program must fail");
    assert_eq!(
        Some(&redblue::Span::new(1, 5)),
        expected.span(),
        "the lexer span moved; update the pinned column"
    );
}

#[test]
fn diagnostics_surface_a_spanned_parser_error() {
    let source = "set x to 1\nsay x +\n";

    let diagnostic = error_diagnostic(source);

    assert_eq!(diagnostic.severity, Severity::Error);
    assert_eq!(
        (diagnostic.line, diagnostic.column),
        (2, 8),
        "the parser diagnostic must point at the offending token"
    );
    assert!(
        !diagnostic.message.is_empty(),
        "a parser diagnostic must name the failure"
    );
}

#[test]
fn diagnostics_surface_a_spanned_analyzer_error() {
    let source = "set x to 1\nsay missing\n";

    let diagnostic = error_diagnostic(source);

    assert_eq!(
        (diagnostic.line, diagnostic.column),
        (2, 1),
        "the analyzer diagnostic must point at the statement holding the bad name"
    );
    assert!(
        diagnostic.message.contains("missing"),
        "the diagnostic must name the offending identifier, got: {}",
        diagnostic.message
    );
}

#[test]
fn diagnostics_report_linter_findings_as_warnings() {
    // A program that runs fine but leaves a variable behind: nothing here is an
    // error, so the linter finding must reach the editor as a warning.
    let source = "set x to 1\n";

    let (lint_errors, lint_warnings) = redblue::linter::lint(source);
    assert_eq!(
        (lint_errors.len(), lint_warnings.len()),
        (0, 1),
        "the fixture must be valid with exactly one unused variable: {:?} / {:?}",
        lint_errors,
        lint_warnings
    );
    assert_eq!(
        (lint_warnings[0].line, lint_warnings[0].column),
        (0, 0),
        "the linter reports a whole-program finding with no position"
    );

    let found = diagnostics(source);

    assert_eq!(
        found.len(),
        1,
        "one lint finding means one diagnostic: {:?}",
        found
    );

    let warning = &found[0];
    assert_eq!(
        warning.severity,
        Severity::Warning,
        "a lint finding must not be reported as an error, or `rb diagnostics` \
         would exit non-zero on a program that runs"
    );
    assert_ne!(
        warning.severity,
        Severity::Error,
        "the warning path must stay distinguishable from the error path"
    );
    assert_eq!(
        (warning.line, warning.column),
        (1, 1),
        "the unplaceable 0:0 finding must fold to the first character"
    );
    assert!(
        warning.message.contains("Unused variable") && warning.message.contains('x'),
        "the warning must name the unused variable, got: {}",
        warning.message
    );

    // The same single finding reaches the JSON an editor consumes, still as a
    // warning rather than an error.
    let value: serde_json::Value =
        serde_json::from_str(&diagnostics_json(source)).expect("diagnostics are not JSON");
    let entries = value.as_array().expect("not an array");
    assert_eq!(entries.len(), 1, "one warning, one JSON entry");
    assert_eq!(entries[0]["severity"].as_str(), Some("warning"));
    assert_eq!(entries[0]["line"].as_u64(), Some(1));
    assert_eq!(entries[0]["column"].as_u64(), Some(1));
    assert_eq!(
        entries[0]["message"].as_str(),
        Some(warning.message.as_str()),
        "the JSON must render the very finding `diagnostics` returned"
    );
}

#[test]
fn unused_variable_warnings_come_out_in_a_stable_name_order() {
    // Four unused variables: the linter collects them from a `HashSet`, whose
    // iteration order is per-instance random, so an unsorted implementation
    // produces a different order on almost every call. An editor diffing the
    // diagnostics between two saves would see the same file churn.
    let source = "set zeta to 1\nset alpha to 2\nset mid to 3\nset beta to 4\n";
    let expected = vec![
        "Unused variable: 'alpha'",
        "Unused variable: 'beta'",
        "Unused variable: 'mid'",
        "Unused variable: 'zeta'",
    ];

    let messages =
        |found: &[Diagnostic]| -> Vec<String> { found.iter().map(|d| d.message.clone()).collect() };

    for attempt in 0..64 {
        let found = diagnostics(source);
        assert_eq!(
            found.len(),
            expected.len(),
            "attempt {}: every variable is unused, so every one is reported: {:?}",
            attempt,
            found
        );
        assert_eq!(
            messages(&found),
            expected,
            "attempt {}: unused-variable findings must be sorted by name so the \
             diagnostics JSON is reproducible",
            attempt
        );
        assert!(
            found.iter().all(|d| d.severity == Severity::Warning),
            "attempt {}: unused variables are warnings, not errors: {:?}",
            attempt,
            found
        );
    }

    // The same order has to hold for the linter's own entry point and for the
    // rendered JSON, which is what `rb diagnostics` prints.
    let (_, warnings) = redblue::linter::lint(source);
    assert_eq!(
        warnings
            .iter()
            .map(|w| w.message.clone())
            .collect::<Vec<_>>(),
        expected,
        "`redblue::linter::lint` must report the same order `diagnostics` does"
    );
    assert_eq!(
        diagnostics_json(source),
        redblue::lsp::diagnostics_to_json(&diagnostics(source)),
        "the JSON must render exactly the findings `diagnostics` returned"
    );
}

#[test]
fn diagnostics_lint_the_ast_they_already_parsed() {
    // `diagnostics` lexes and parses once for its own findings; the linter must
    // read that AST instead of taking the source and running the frontend a
    // second time. Linting the parsed program has to give the same findings.
    let source = "set kept to 1\nsay kept\nset dropped to 2\n";

    let tokens = Lexer::tokenize(source).expect("the fixture must lex");
    let program = redblue::parser::parse(tokens).expect("the fixture must parse");
    let (lint_errors, lint_warnings) = redblue::linter::lint_program(&program);

    assert!(
        lint_errors.is_empty(),
        "the fixture has no lint errors: {:?}",
        lint_errors
    );

    let from_ast: Vec<String> = lint_warnings.iter().map(|w| w.message.clone()).collect();
    let reported: Vec<String> = diagnostics(source)
        .iter()
        .map(|d| d.message.clone())
        .collect();

    assert_eq!(
        reported, from_ast,
        "linting the already-parsed program must produce the diagnostics the \
         editor sees, in the same order"
    );
    assert_eq!(
        reported,
        vec!["Unused variable: 'dropped'".to_string()],
        "the fixture must report exactly its one unused variable"
    );
}

#[test]
fn edge_empty_source_produces_no_diagnostics() {
    assert!(
        diagnostics("").is_empty(),
        "an empty file is an empty program, not a failure: {:?}",
        diagnostics("")
    );
    assert_eq!(
        diagnostics_json(""),
        "[]",
        "an empty program must render as an empty JSON array"
    );
}

#[test]
fn edge_diagnostic_columns_count_characters_not_bytes() {
    // On the second line the two multi-byte characters sit before the literal:
    // the emoji is one character but four bytes, the e-acute two bytes. A
    // byte-counting column would report 14 instead of 10.
    let source = "say \"ok\"\nsay \"\u{e9}\u{1F600}\" 1.2.3\n";

    let diagnostic = error_diagnostic(source);

    assert_eq!(
        (diagnostic.line, diagnostic.column),
        (2, 10),
        "columns must count characters so the editor caret lines up"
    );
    assert_ne!(
        diagnostic.column, 14,
        "the byte offset must not leak into the column"
    );
    assert!(
        diagnostic.message.contains("Invalid number"),
        "an invalid number literal must be diagnosed, got: {}",
        diagnostic.message
    );
}

#[test]
fn edge_escape_json_escapes_quotes_backslashes_controls_and_unicode() {
    let text = "a\"b\\c\nd\te\u{1}f\u{1F600}";

    let escaped = redblue::lsp::escape_json(text);

    let round_tripped: String =
        serde_json::from_str(&escaped).expect("escape_json must emit valid JSON");

    assert_eq!(
        round_tripped, text,
        "an escaped string must read back as the original: {}",
        escaped
    );
    assert!(
        escaped.contains("\\u0001"),
        "a control character must be \\u-escaped, got: {}",
        escaped
    );
}

#[test]
fn diagnostics_json_is_a_well_formed_array_of_located_entries() {
    let source = "say \"a\\\"b\n";
    let json = diagnostics_json(source);

    let value: serde_json::Value =
        serde_json::from_str(&json).unwrap_or_else(|e| panic!("diagnostics are not JSON: {}", e));
    let entries = value
        .as_array()
        .unwrap_or_else(|| panic!("not an array: {}", json));

    assert_eq!(
        entries.len(),
        1,
        "one failure means one diagnostic: {}",
        json
    );
    assert_eq!(entries[0]["severity"].as_str(), Some("error"));
    assert_eq!(entries[0]["line"].as_u64(), Some(1));
    assert_eq!(entries[0]["column"].as_u64(), Some(5));
    assert_eq!(
        entries[0]["message"].as_str(),
        Some(error_diagnostic(source).message.as_str()),
        "the JSON message must be the same single-line string the diagnostic carries"
    );
}

#[test]
fn edge_diagnostic_json_escapes_a_message_containing_quotes_and_newlines() {
    let diagnostic = Diagnostic {
        line: 3,
        column: 4,
        message: "he said \"hi\"\nthen left\tend".to_string(),
        severity: Severity::Warning,
    };

    let value: serde_json::Value = serde_json::from_str(&diagnostic.to_json())
        .unwrap_or_else(|e| panic!("a diagnostic is not valid JSON: {}", e));

    assert_eq!(value["line"].as_u64(), Some(3));
    assert_eq!(value["column"].as_u64(), Some(4));
    assert_eq!(value["severity"].as_str(), Some("warning"));
    assert_eq!(
        value["message"].as_str(),
        Some("he said \"hi\"\nthen left\tend"),
        "quotes, newlines and tabs must survive the round trip"
    );
}
