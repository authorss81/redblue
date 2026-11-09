//! Editor/LSP tooling: the shipped TextMate grammar must cover the keyword set
//! the lexer actually recognises, and diagnostics must surface the spanned
//! lexer/parser/analyzer errors `run_source` reports.

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use redblue::lexer::Lexer;
use redblue::lsp::{
    completions, completions_json, diagnostics, diagnostics_json, find_references,
    find_references_json, goto_definition, goto_definition_json, keyword_pattern, textmate_grammar,
    Diagnostic, Severity, GRAMMAR_PATH, LANGUAGE_CONFIG_PATH,
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
        Some(&redblue::Span::new(1, 5).extended_to(2, 1)),
        expected.span(),
        "the lexer span moved; update the pinned column. The opening quote is \
         still where it was, and the span now also records the end of input the \
         runaway string reached"
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
        (2, 5),
        "the analyzer diagnostic must point at the name holding the failure — \
         the editor underlines the identifier, not the statement around it"
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
        (1, 1),
        "the linter must place a finding at the line that defined the variable, \
         so the editor can underline it"
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
    //
    // The stable order is the order of the source, because every finding now
    // carries the line that defined it and a reader scans a file top to bottom.
    // Where two findings share a line, the name breaks the tie. Name order on
    // its own would make the reported line numbers jump around.
    let source = "set zeta to 1\nset alpha to 2\nset mid to 3\nset beta to 4\n";
    let expected = vec![
        "Unused variable: 'zeta'",
        "Unused variable: 'alpha'",
        "Unused variable: 'mid'",
        "Unused variable: 'beta'",
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
            "attempt {}: unused-variable findings must be sorted so the \
             diagnostics JSON is reproducible",
            attempt
        );
        assert!(
            found
                .windows(2)
                .all(|pair| { (pair[0].line, pair[0].column) < (pair[1].line, pair[1].column) }),
            "attempt {}: findings must be ordered by position, which is what \
             makes the order total: {:?}",
            attempt,
            found
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

// ---------------------------------------------------------------------------
// Navigation: goto-definition, find-references and completion are pure
// functions over source text, so a test pins exact positions rather than
// trusting a server loop.
// ---------------------------------------------------------------------------

#[test]
fn goto_definition_on_variable_use_returns_declaration_span() {
    let source = "set greeting to \"hi\"\nsay greeting\n";

    let found = goto_definition(source, 2, 5)
        .expect("a position inside the file is valid")
        .expect("the use of `greeting` names a binding");

    assert_eq!(
        (found.line, found.column, found.kind.as_str()),
        (1, 5, "variable"),
        "the use must point at the `set` name, not at the use itself"
    );
    assert_eq!(found.name, "greeting");
}

#[test]
fn edge_goto_definition_on_builtin_resolves_to_registry_not_file_span() {
    let source = "say abs(-3)\n";

    let found = goto_definition(source, 1, 5)
        .expect("a position inside the file is valid")
        .expect("`abs` is a builtin this crate answers");

    assert_eq!(found.kind.as_str(), "builtin");
    assert_eq!(found.name, "abs");
    assert_eq!(
        (found.line, found.column),
        (0, 0),
        "a builtin has no file span: line 0 says resolution came from the registry"
    );
}

#[test]
fn find_references_distinguishes_shadowed_bindings() {
    let source = concat!(
        "set x to 1\n",
        "say x\n",
        "to show(x)\n",
        "    say x\n",
        "end\n",
        "say x\n",
    );

    let outer: Vec<(usize, usize)> = find_references(source, 1, 5)
        .expect("a position inside the file is valid")
        .into_iter()
        .map(|location| (location.line, location.column))
        .collect();
    let inner: Vec<(usize, usize)> = find_references(source, 3, 9)
        .expect("a position inside the file is valid")
        .into_iter()
        .map(|location| (location.line, location.column))
        .collect();

    assert_eq!(
        outer,
        vec![(1, 5), (2, 5), (6, 5)],
        "the outer binding's references must not include the parameter or its body"
    );
    assert_eq!(
        inner,
        vec![(3, 9), (4, 9)],
        "the parameter's references must not include the outer binding"
    );
}

#[test]
fn edge_find_references_on_imported_module_spans_the_import() {
    let source = "import MathUtils\nsay MathUtils.circle_area(2)\n";

    let definition = goto_definition(source, 2, 5)
        .expect("a position inside the file is valid")
        .expect("the imported module name is bound by the import");

    assert_eq!(
        (definition.line, definition.column, definition.kind.as_str()),
        (1, 8, "import"),
        "the module name resolves to the import that bound it"
    );

    let references: Vec<(usize, usize)> = find_references(source, 2, 5)
        .expect("a position inside the file is valid")
        .into_iter()
        .map(|location| (location.line, location.column))
        .collect();

    assert_eq!(
        references,
        vec![(1, 8), (2, 5)],
        "the import declaration is the binding site, not a module definition file"
    );
}

#[test]
fn edge_empty_source_answers_without_failing() {
    let source = "";

    assert_eq!(
        goto_definition(source, 1, 1).expect("the empty file has one position"),
        None,
        "there is no name to jump to in an empty file"
    );
    assert!(
        find_references(source, 1, 1)
            .expect("the empty file has one position")
            .is_empty(),
        "an empty file has no references"
    );

    let offered = completions(source, 1, 1).expect("the empty file has one position");
    for expected in ["set", "abs"] {
        assert!(
            offered.contains(&expected.to_string()),
            "an empty file still offers the language vocabulary: missing {expected:?}"
        );
    }
}

#[test]
fn edge_goto_definition_counts_unicode_names_as_characters() {
    let source = "set café to 1\nsay café\n";

    let found = goto_definition(source, 2, 5)
        .expect("the use is inside the file")
        .expect("a Unicode identifier is a name like any other");

    assert_eq!(
        (
            found.line,
            found.column,
            found.end_column,
            found.kind.as_str()
        ),
        (1, 5, 8, "variable"),
        "the span must cover four characters, not the five bytes `café` occupies"
    );
}

#[test]
fn edge_completions_at_eof_without_trailing_newline_include_keywords_builtins_and_scope() {
    let source = "set alpha to 1";

    let offered = completions(source, 1, 15).expect("the end-of-file position is valid");

    for expected in ["alpha", "set", "abs"] {
        assert!(
            offered.contains(&expected.to_string()),
            "completion at EOF must offer {expected:?}, got {offered:?}"
        );
    }
}

#[test]
fn edge_completions_inside_string_literal_are_empty() {
    let source = "say \"hello\"\n";

    let offered = completions(source, 1, 7).expect("the position is inside the file");

    assert!(
        offered.is_empty(),
        "a string literal is text, not code: completion must offer nothing, got {offered:?}"
    );
}

#[test]
fn edge_navigation_handles_five_thousand_lines_quickly() {
    let mut source = String::new();
    for index in 0..5000 {
        source.push_str(&format!("set v{index} to {index}\n"));
    }
    let last_line = 5000;
    let last_column = 5;
    let eof_column = source
        .lines()
        .last()
        .map(|line| line.chars().count() + 1)
        .unwrap();

    let started = Instant::now();
    let definition = goto_definition(&source, last_line, last_column)
        .expect("the last line is inside the file")
        .expect("the last declaration names a binding");
    let references = find_references(&source, last_line, last_column).expect("valid position");
    let offered = completions(&source, last_line, eof_column).expect("valid position");
    let elapsed = started.elapsed();

    assert_eq!(
        (definition.line, definition.column),
        (last_line, last_column)
    );
    assert!(!references.is_empty(), "a declaration is its own reference");
    assert!(
        offered.contains(&"v4999".to_string()),
        "the last binding is in scope at EOF"
    );
    assert!(
        elapsed < Duration::from_secs(30),
        "navigation over 5000 lines must answer promptly, took {elapsed:?}"
    );
}

#[test]
fn edge_loop_and_object_scopes_do_not_pop_the_enclosing_function_scope() {
    let source = concat!(
        "to show(x)\n",
        "    while x > 0\n",
        "        say x\n",
        "    end\n",
        "    object Person\n",
        "        has name\n",
        "    end\n",
        "    say x\n",
        "end\n",
        "say x\n",
    );

    let after_blocks = goto_definition(source, 8, 9)
        .expect("a position inside the file is valid")
        .expect("the parameter is still in scope after the loop and object close");
    assert_eq!(
        (
            after_blocks.line,
            after_blocks.column,
            after_blocks.kind.as_str()
        ),
        (1, 9, "parameter"),
        "a loop or object `end` must close its own scope, not the function's"
    );

    assert_eq!(
        goto_definition(source, 10, 5).expect("a position inside the file is valid"),
        None,
        "the parameter must not leak past the function's own `end`"
    );
}

#[test]
fn edge_for_loop_variable_does_not_leak_into_the_enclosing_scope() {
    let source = concat!(
        "for each item in items\n",
        "    say item\n",
        "end\n",
        "say item\n",
    );

    let inside = goto_definition(source, 2, 9)
        .expect("a position inside the file is valid")
        .expect("the loop variable is in scope inside the loop body");
    assert_eq!(
        (inside.line, inside.column, inside.kind.as_str()),
        (1, 10, "loop"),
        "the body's `item` must resolve to the loop variable"
    );

    assert_eq!(
        goto_definition(source, 4, 5).expect("a position inside the file is valid"),
        None,
        "the loop variable must not leak past the loop's `end`"
    );
}

#[test]
fn edge_else_if_inside_a_function_keeps_the_function_scope_balanced() {
    let source = concat!(
        "to pick(flag)\n",
        "    if flag is 1 then\n",
        "        say flag\n",
        "    else if flag is 2 then\n",
        "        say flag\n",
        "    end\n",
        "    say flag\n",
        "end\n",
        "say flag\n",
    );

    let inside = goto_definition(source, 7, 9)
        .expect("a position inside the file is valid")
        .expect("the parameter is still in scope after the else-if chain closes");
    assert_eq!(
        (inside.line, inside.column, inside.kind.as_str()),
        (1, 9, "parameter"),
        "an `else if` must not leave a stale frame that steals the function's `end`"
    );

    assert_eq!(
        goto_definition(source, 9, 5).expect("a position inside the file is valid"),
        None,
        "the parameter must not leak past the function's own `end`"
    );
}

#[test]
fn edge_multiple_assignment_declares_every_name_before_to() {
    let source = "set a, b to 1, 2\nsay a\nsay b\n";

    let first = goto_definition(source, 2, 5)
        .expect("a position inside the file is valid")
        .expect("the first assigned name is declared");
    let second = goto_definition(source, 3, 5)
        .expect("a position inside the file is valid")
        .expect("the second assigned name is declared");

    assert_eq!(
        (first.line, first.column, first.kind.as_str()),
        (1, 5, "variable")
    );
    assert_eq!(
        (second.line, second.column, second.kind.as_str()),
        (1, 8, "variable"),
        "`set a, b to ...` declares `b` as well as `a`"
    );

    let references: Vec<(usize, usize)> = find_references(source, 3, 5)
        .expect("a position inside the file is valid")
        .into_iter()
        .map(|location| (location.line, location.column))
        .collect();
    assert_eq!(references, vec![(1, 8), (3, 5)]);
}

#[test]
fn edge_typed_and_of_parameters_do_not_declare_their_type_names() {
    let source = concat!(
        "to total(items of list)\n",
        "    give back length(items)\n",
        "end\n",
        "to scale(value: number)\n",
        "    give back value * 2\n",
        "end\n",
    );

    let items = goto_definition(source, 2, 22)
        .expect("a position inside the file is valid")
        .expect("`items` is a parameter");
    assert_eq!(
        (items.line, items.column, items.kind.as_str()),
        (1, 10, "parameter")
    );

    let list = goto_definition(source, 1, 19)
        .expect("a position inside the file is valid")
        .expect("`list` is a module name, not a parameter");
    assert_eq!(
        (list.kind.as_str(), list.name.as_str()),
        ("builtin", "list"),
        "the `of list` type must not declare `list`"
    );

    assert_eq!(
        goto_definition(source, 4, 17).expect("a position inside the file is valid"),
        None,
        "the `number` type must not declare a parameter"
    );
}

#[test]
fn edge_catch_binders_are_the_name_after_the_handler_word() {
    let plain = concat!(
        "try\n",
        "    say 1 / 0\n",
        "catch problem\n",
        "    say problem\n",
        "end\n",
    );

    let plain_binder = goto_definition(plain, 4, 9)
        .expect("a position inside the file is valid")
        .expect("`problem` names the caught failure");
    assert_eq!(
        (
            plain_binder.line,
            plain_binder.column,
            plain_binder.kind.as_str()
        ),
        (3, 7, "catch"),
        "`catch problem` binds `problem`, not the word after `catch`"
    );

    let plain_references: Vec<(usize, usize)> = find_references(plain, 3, 7)
        .expect("a position inside the file is valid")
        .into_iter()
        .map(|location| (location.line, location.column))
        .collect();
    assert_eq!(plain_references, vec![(3, 7), (4, 9)]);

    let typed = concat!(
        "try\n",
        "    say 1 / 0\n",
        "catch error of Runtime problem\n",
        "    say problem\n",
        "end\n",
    );

    let typed_binder = goto_definition(typed, 4, 9)
        .expect("a position inside the file is valid")
        .expect("the name after the failure kind is the binder");
    assert_eq!(
        (
            typed_binder.line,
            typed_binder.column,
            typed_binder.kind.as_str()
        ),
        (3, 24, "catch"),
        "`catch error of Runtime problem` binds `problem`, not `error`"
    );

    let typed_references: Vec<(usize, usize)> = find_references(typed, 3, 24)
        .expect("a position inside the file is valid")
        .into_iter()
        .map(|location| (location.line, location.column))
        .collect();
    assert_eq!(typed_references, vec![(3, 24), (4, 9)]);
}

#[test]
fn edge_membership_has_does_not_declare_the_name_after_it() {
    let source = "if functions has abs then\n    say abs(-1)\nend\n";

    let found = goto_definition(source, 2, 9)
        .expect("a position inside the file is valid")
        .expect("`abs` is still the builtin");

    assert_eq!(
        (found.kind.as_str(), found.name.as_str()),
        ("builtin", "abs"),
        "the membership `has` must not bind the name on its right"
    );
}

#[test]
fn edge_range_with_a_call_lower_bound_does_not_push_a_function_frame() {
    let source = concat!(
        "to finish()\n",
        "    give back 3\n",
        "end\n",
        "set limit to 3\n",
        "for each i from finish() to limit\n",
        "    say i\n",
        "end\n",
        "say i\n",
    );

    let inside = goto_definition(source, 6, 9)
        .expect("a position inside the file is valid")
        .expect("the loop variable is in scope inside the range loop");
    assert_eq!(
        (inside.line, inside.column, inside.kind.as_str()),
        (5, 10, "loop"),
        "a range's `to` is not a function declaration"
    );

    assert_eq!(
        goto_definition(source, 8, 5).expect("a position inside the file is valid"),
        None,
        "the loop variable must not leak past the range loop's `end`"
    );
}

#[test]
fn malformed_positions_are_clean_errors_not_panics() {
    let source = "set x to 1\n";

    for (line, column) in [(0, 1), (1, 0), (1, 99), (99, 1)] {
        assert!(
            goto_definition(source, line, column).is_err(),
            "goto_definition must reject {line}:{column} with an error"
        );
        assert!(
            find_references(source, line, column).is_err(),
            "find_references must reject {line}:{column} with an error"
        );
        assert!(
            completions(source, line, column).is_err(),
            "completions must reject {line}:{column} with an error"
        );
    }
}

#[test]
fn edge_malformed_source_returns_errors_not_panics() {
    let source = "say \"unterminated";

    assert!(
        goto_definition(source, 1, 1).is_err(),
        "goto_definition must surface a lexer error as Err, not panic or succeed"
    );
    assert!(
        find_references(source, 1, 1).is_err(),
        "find_references must surface a lexer error as Err, not panic or succeed"
    );
    assert!(
        completions(source, 1, 1).is_err(),
        "completions must surface a lexer error as Err, not panic or succeed"
    );
}

#[test]
fn navigation_json_extends_the_line_column_shape() {
    let source = "set x to 1\nsay x\n";

    let definition: serde_json::Value =
        serde_json::from_str(&goto_definition_json(source, 2, 5).expect("valid position"))
            .expect("goto-definition JSON must parse");
    assert_eq!(definition["line"], 1);
    assert_eq!(definition["column"], 5);
    assert_eq!(definition["kind"], "variable");

    let references: serde_json::Value =
        serde_json::from_str(&find_references_json(source, 1, 5).expect("valid position"))
            .expect("find-references JSON must parse");
    assert_eq!(references[0]["line"], 1);
    assert_eq!(references[1]["line"], 2);

    let offered: serde_json::Value =
        serde_json::from_str(&completions_json(source, 2, 6).expect("valid position"))
            .expect("completion JSON must parse");
    assert_eq!(offered["line"], 2);
    assert_eq!(offered["column"], 6);
    assert!(offered["items"]
        .as_array()
        .expect("items is an array")
        .iter()
        .any(|value| value == "x"));
}
