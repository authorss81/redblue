//! Editor/LSP tooling: the shipped TextMate grammar must cover the keyword set
//! the lexer actually recognises, and diagnostics must surface the spanned
//! lexer/parser/analyzer errors `run_source` reports.

use std::fs;
use std::path::PathBuf;

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
fn edge_nested_import_stays_in_its_block() {
    let source = concat!(
        "to load()\n",
        "    import MathUtils\n",
        "    say MathUtils\n",
        "end\n",
        "say MathUtils\n",
    );

    let inside = goto_definition(source, 3, 9)
        .expect("a position inside the file is valid")
        .expect("the import binds its module inside the function body");
    assert_eq!(
        (inside.line, inside.column, inside.kind.as_str()),
        (2, 12, "import"),
        "the body's use resolves to the import written in the same scope"
    );

    assert_eq!(
        goto_definition(source, 5, 5).expect("a position inside the file is valid"),
        None,
        "an import declared in a function body must not leak past the function's `end`"
    );
}

#[test]
fn edge_import_alias_keeps_the_modules_own_name_a_binding() {
    // `import MathUtils as M` binds both spellings: the runtime and analyzer
    // leave the module's own name readable (`import json as J` then
    // `json.stringify(..)` still works), so navigation must offer both and
    // keep their references apart.
    let source = concat!(
        "import MathUtils as M\n",
        "say MathUtils.circle_area(2)\n",
        "say M.circle_area(2)\n",
    );

    for (line, column, expected_column) in [(2, 5, 8), (3, 5, 21)] {
        let definition = goto_definition(source, line, column)
            .expect("a position inside the file is valid")
            .expect("both the module name and its alias name the import");
        assert_eq!(
            (definition.line, definition.column, definition.kind.as_str()),
            (1, expected_column, "import"),
            "line {line} resolves to its own import binding"
        );
    }

    let module_references: Vec<(usize, usize)> = find_references(source, 2, 5)
        .expect("a position inside the file is valid")
        .into_iter()
        .map(|location| (location.line, location.column))
        .collect();
    assert_eq!(
        module_references,
        vec![(1, 8), (2, 5)],
        "the module's own name has only its import declaration and uses"
    );

    let alias_references: Vec<(usize, usize)> = find_references(source, 3, 5)
        .expect("a position inside the file is valid")
        .into_iter()
        .map(|location| (location.line, location.column))
        .collect();
    assert_eq!(
        alias_references,
        vec![(1, 21), (3, 5)],
        "the alias has only its import declaration and uses"
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
fn edge_completions_at_eof_after_a_one_line_literal_exclude_its_parameters() {
    let source = "set f to to (literal_param) give back literal_param";
    let eof_column = source.chars().count() + 1;

    let offered = completions(source, 1, eof_column).expect("the end-of-file position is valid");

    assert!(
        !offered.contains(&"literal_param".to_string()),
        "a single-line literal ends at the final line, so its parameters must not still be in \
         scope at EOF, got {offered:?}"
    );
}

#[test]
fn edge_reference_after_a_one_line_literal_resolves_to_the_outer_binding() {
    let source = concat!(
        "set x to 100\n",
        "to f(p)\n",
        "    give back p\n",
        "end\n",
        "say f(to (x) give back x, x)\n",
    );

    let trailing = goto_definition(source, 5, 27)
        .expect("the trailing argument is inside the file")
        .expect("the trailing `x` names the outer binding");

    assert_eq!(
        (trailing.line, trailing.column, trailing.kind.as_str()),
        (1, 5, "variable"),
        "the literal's parameter must not capture an argument written after the literal"
    );

    let outer: Vec<(usize, usize)> = find_references(source, 1, 5)
        .expect("a position inside the file is valid")
        .into_iter()
        .map(|location| (location.line, location.column))
        .collect();
    let inner: Vec<(usize, usize)> = find_references(source, 5, 11)
        .expect("a position inside the file is valid")
        .into_iter()
        .map(|location| (location.line, location.column))
        .collect();

    assert_eq!(
        outer,
        vec![(1, 5), (5, 27)],
        "the outer binding must include the trailing argument, not the literal's parameter"
    );
    assert_eq!(
        inner,
        vec![(5, 11), (5, 24)],
        "the literal's parameter must own only its body, not the trailing argument"
    );
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
fn edge_completions_inside_line_comment_are_empty() {
    let source = "set value to 1\nsay value // a note here\n";

    let offered = completions(source, 2, 19).expect("the position is inside the file");

    assert!(
        offered.is_empty(),
        "a comment is not code: completion inside it must offer nothing, got {offered:?}"
    );
}

#[test]
fn edge_completions_at_the_end_of_a_line_comment_are_empty() {
    let source = "set value to 1\nsay value // a note here\n";
    let end_column = source
        .lines()
        .nth(1)
        .map(|line| line.chars().count() + 1)
        .unwrap();

    let offered = completions(source, 2, end_column).expect("the end of the comment is valid");

    assert!(
        offered.is_empty(),
        "the terminating newline is still part of the comment line: completion must offer \
         nothing, got {offered:?}"
    );
}

#[test]
fn edge_completions_at_eof_inside_an_unterminated_comment_are_empty() {
    let source = "set value to 1\nsay value // a note here";
    let end_column = source
        .lines()
        .nth(1)
        .map(|line| line.chars().count() + 1)
        .unwrap();

    let offered = completions(source, 2, end_column).expect("the end of the file is valid");

    assert!(
        offered.is_empty(),
        "an unterminated comment runs to EOF, so completion at EOF must offer nothing, got \
         {offered:?}"
    );
}

#[test]
fn edge_completions_after_a_line_comment_still_offer_scope() {
    let source = "set value to 1 // a note\nsay val\n";

    let offered = completions(source, 2, 8).expect("the end of the second line is valid");

    assert!(
        offered.contains(&"value".to_string()),
        "a comment on the previous line must not close the scope, got {offered:?}"
    );
}

#[test]
fn edge_completions_do_not_treat_slashes_inside_text_as_a_comment() {
    let source = "set value to 1\nsay \"http://x\" then value\n";

    let offered = completions(source, 2, 22).expect("the position is inside the file");

    assert!(
        offered.contains(&"value".to_string()),
        "`//` inside a text literal is text, not a comment: it must not suppress completion, \
         got {offered:?}"
    );
}

#[test]
fn edge_navigation_rejects_a_column_past_the_line() {
    let source = "set x to 1\n";
    let past = source
        .lines()
        .next()
        .map(|line| line.chars().count() + 2)
        .unwrap();

    assert!(
        goto_definition(source, 1, past).is_err(),
        "a column past the line must be a clean error"
    );
    assert!(
        find_references(source, 1, past).is_err(),
        "a column past the line must be a clean error"
    );
    assert!(
        completions(source, 1, past).is_err(),
        "a column past the line must be a clean error"
    );
}

fn navigation_source(lines: usize) -> String {
    let mut source = String::new();
    for index in 0..lines {
        source.push_str(&format!("set v{index} to {index}\n"));
    }
    source
}

#[test]
fn edge_navigation_resolves_a_binding_after_five_thousand_lines() {
    // A wall-clock ratio between two samples is nondeterministic on a loaded
    // runner. The resource bound is the input itself: a 5000-declaration file
    // must answer every capability at its last line, and the operation count
    // is fixed by the source rather than by the machine's speed.
    let lines = 5000;
    let source = navigation_source(lines);
    let last_line = lines;
    let last_column = 5;
    let eof_column = source
        .lines()
        .last()
        .map(|line| line.chars().count() + 1)
        .unwrap();

    let definition = goto_definition(&source, last_line, last_column)
        .expect("the last line is inside the file")
        .expect("the last declaration names a binding");
    assert_eq!(
        (definition.line, definition.column),
        (last_line, last_column)
    );

    let references = find_references(&source, last_line, last_column).expect("valid position");
    assert_eq!(
        references
            .iter()
            .map(|location| (location.line, location.column))
            .collect::<Vec<_>>(),
        vec![(last_line, last_column)],
        "the last declaration is its only reference"
    );

    let offered = completions(&source, last_line, eof_column).expect("valid position");
    assert!(
        offered.contains(&format!("v{}", lines - 1)),
        "the last binding is in scope at EOF"
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
fn edge_unless_body_set_survives_the_end_like_the_interpreter() {
    // A `set` inside an `unless` body writes to the program scope, so it is
    // readable after `end`; a function or import declared there belongs to the
    // body's own scope. Navigation models both, because that is what the
    // analyzer accepts and the VM runs.
    let source = concat!(
        "set marker to 1\n",
        "unless no then\n",
        "    set local to 2\n",
        "    say local\n",
        "end\n",
        "say marker\n",
        "say local\n",
    );

    let inside = goto_definition(source, 4, 9)
        .expect("a position inside the file is valid")
        .expect("the set is in scope inside the unless body");
    assert_eq!(
        (inside.line, inside.column, inside.kind.as_str()),
        (3, 9, "variable"),
        "the body's use must resolve to the `set` it reads"
    );

    let after = goto_definition(source, 6, 5)
        .expect("a position inside the file is valid")
        .expect("the enclosing binding is still in scope after the unless closes");
    assert_eq!(
        (after.line, after.column, after.kind.as_str()),
        (1, 5, "variable"),
        "the enclosing binding must survive the unless `end`"
    );

    let local = goto_definition(source, 7, 5)
        .expect("a position inside the file is valid")
        .expect("a first assignment inside the body is a program binding");
    assert_eq!(
        (local.line, local.column, local.kind.as_str()),
        (3, 9, "variable"),
        "the VM writes the body's first assignment to the program scope, so it outlives `end`"
    );
}

#[test]
fn edge_function_declared_in_a_branch_body_does_not_leak_past_end() {
    // The analyzer pushes and pops a scope around an `if` or `unless` body, so
    // a function declared there is not a name of the program after `end`.
    for source in [
        concat!(
            "unless no then\n",
            "    to helper\n",
            "        give back 1\n",
            "    end\n",
            "end\n",
            "say helper\n",
        ),
        concat!(
            "if yes then\n",
            "    to helper\n",
            "        give back 1\n",
            "    end\n",
            "end\n",
            "say helper\n",
        ),
    ] {
        assert_eq!(
            goto_definition(source, 6, 5).expect("a position inside the file is valid"),
            None,
            "a branch body is its own scope, so the function does not leak past `end`"
        );
    }
}

#[test]
fn edge_while_scope_does_not_leak_into_the_enclosing_scope() {
    let source = concat!(
        "set marker to 1\n",
        "while marker is 1\n",
        "    to helper\n",
        "        give back marker\n",
        "    end\n",
        "    say helper()\n",
        "end\n",
        "say marker\n",
        "say helper\n",
    );

    let inside = goto_definition(source, 6, 9)
        .expect("a position inside the file is valid")
        .expect("the helper is in scope inside the while body");
    assert_eq!(
        (inside.line, inside.column, inside.kind.as_str()),
        (3, 8, "function"),
        "the helper declaration must resolve inside its own `while` body"
    );

    let after = goto_definition(source, 8, 5)
        .expect("a position inside the file is valid")
        .expect("the enclosing binding is still in scope after the loop closes");
    assert_eq!(
        (after.line, after.column, after.kind.as_str()),
        (1, 5, "variable"),
        "the while `end` must close only its own scope"
    );
    assert_eq!(
        goto_definition(source, 9, 5).expect("a position inside the file is valid"),
        None,
        "a declaration inside a `while` body must not leak past its `end`"
    );
}

#[test]
fn edge_repeat_scopes_do_not_leak_into_the_enclosing_scope() {
    let counted = concat!(
        "set marker to 1\n",
        "repeat 2\n",
        "    to helper\n",
        "        give back marker\n",
        "    end\n",
        "    say helper()\n",
        "end\n",
        "say marker\n",
        "say helper\n",
    );
    let until = concat!(
        "set marker to 1\n",
        "repeat\n",
        "    to helper\n",
        "        give back marker\n",
        "    end\n",
        "    say helper()\n",
        "until marker is 1\n",
        "say marker\n",
        "say helper\n",
    );

    for (form, source) in [("counted repeat", counted), ("repeat until", until)] {
        let inside = goto_definition(source, 6, 9)
            .unwrap_or_else(|error| panic!("{form}: the helper use is valid: {error}"))
            .unwrap_or_else(|| panic!("{form}: the helper is in scope inside the repeat body"));
        assert_eq!(
            (inside.line, inside.column, inside.kind.as_str()),
            (3, 8, "function"),
            "{form}: the helper declaration must resolve inside its own repeat body"
        );

        let after = goto_definition(source, 8, 5)
            .unwrap_or_else(|error| panic!("{form}: the enclosing use is valid: {error}"))
            .unwrap_or_else(|| panic!("{form}: the enclosing binding survives the repeat"));
        assert_eq!(
            (after.line, after.column, after.kind.as_str()),
            (1, 5, "variable"),
            "{form}: the terminator must close only the repeat's own scope"
        );
        assert_eq!(
            goto_definition(source, 9, 5).expect("a position inside the file is valid"),
            None,
            "{form}: a declaration inside the repeat body must not leak past its terminator"
        );
    }
}

#[test]
fn edge_module_scope_does_not_leak_into_the_enclosing_scope() {
    let source = concat!(
        "set marker to 1\n",
        "module Tools\n",
        "    to helper\n",
        "        give back marker\n",
        "    end\n",
        "    say helper()\n",
        "end\n",
        "say Tools\n",
        "say marker\n",
        "say helper\n",
    );

    let inside = goto_definition(source, 6, 9)
        .expect("a position inside the file is valid")
        .expect("the helper is in scope inside the module body");
    assert_eq!(
        (inside.line, inside.column, inside.kind.as_str()),
        (3, 8, "function"),
        "the helper declaration must resolve inside its own module body"
    );

    let module = goto_definition(source, 8, 5)
        .expect("a position inside the file is valid")
        .expect("the module name is still bound after the declaration closes");
    assert_eq!(
        (module.line, module.column, module.kind.as_str()),
        (2, 8, "module"),
        "the module `end` must keep the module name in the enclosing scope"
    );

    let after = goto_definition(source, 9, 5)
        .expect("a position inside the file is valid")
        .expect("the enclosing binding is still in scope after the module closes");
    assert_eq!(
        (after.line, after.column, after.kind.as_str()),
        (1, 5, "variable"),
        "the module `end` must close only its own scope"
    );
    assert_eq!(
        goto_definition(source, 10, 5).expect("a position inside the file is valid"),
        None,
        "a declaration inside a module must not leak past its `end`"
    );
}

#[test]
fn edge_test_block_keeps_the_enclosing_scope_balanced() {
    let source = concat!(
        "set marker to 1\n",
        "test \"scope\"\n",
        "    for each item in values\n",
        "        say item\n",
        "    end\n",
        "    say marker\n",
        "    set local to 2\n",
        "    say local\n",
        "end\n",
        "say marker\n",
        "say local\n",
        "say item\n",
    );

    let item = goto_definition(source, 4, 13)
        .expect("a position inside the file is valid")
        .expect("the loop variable is in scope inside the test body");
    assert_eq!(
        (item.line, item.column, item.kind.as_str()),
        (3, 14, "loop"),
        "the test body's loop variable must resolve inside its own loop"
    );

    let inside = goto_definition(source, 6, 9)
        .expect("a position inside the file is valid")
        .expect("the enclosing binding is in scope inside the test body");
    assert_eq!(
        (inside.line, inside.column, inside.kind.as_str()),
        (1, 5, "variable"),
        "the test body must read the scope it was written in"
    );

    let local = goto_definition(source, 8, 9)
        .expect("a position inside the file is valid")
        .expect("a declaration in the test body is in scope there");
    assert_eq!(
        (local.line, local.column, local.kind.as_str()),
        (7, 9, "variable"),
        "the test body must read the binding it declares"
    );

    let after = goto_definition(source, 10, 5)
        .expect("a position inside the file is valid")
        .expect("the enclosing binding is still in scope after the test closes");
    assert_eq!(
        (after.line, after.column, after.kind.as_str()),
        (1, 5, "variable"),
        "the test `end` must close only its own frame"
    );
    assert_eq!(
        goto_definition(source, 11, 5).expect("a position inside the file is valid"),
        None,
        "a declaration written directly in the test body must not leak past its `end`"
    );
    assert_eq!(
        goto_definition(source, 12, 5).expect("a position inside the file is valid"),
        None,
        "a binding declared inside the test body's block must not leak past its `end`"
    );
}

#[test]
fn edge_unless_body_binding_joins_the_enclosing_bindings_references() {
    let source = concat!(
        "set marker to 1\n",
        "unless no then\n",
        "    set local to 2\n",
        "    say local\n",
        "end\n",
        "say local\n",
    );

    let declared = goto_definition(source, 6, 5)
        .expect("a position inside the file is valid")
        .expect("the unless body's declaration outlives its `end`");
    assert_eq!(
        (declared.line, declared.column, declared.kind.as_str()),
        (3, 9, "variable"),
        "goto after an `unless` must answer the declaration the interpreter still has bound"
    );

    let references: Vec<(usize, usize)> = find_references(source, 6, 5)
        .expect("a position inside the file is valid")
        .into_iter()
        .map(|location| (location.line, location.column))
        .collect();
    assert_eq!(
        references,
        vec![(3, 9), (4, 9), (6, 5)],
        "the declaration and both uses are the same binding"
    );
}

#[test]
fn edge_test_body_declaration_is_not_offered_by_completion_after_end() {
    // The test harness parses each body separately and runs it in an isolated
    // VM, so nothing a test declares survives its `end`, and one test cannot
    // see another's names.
    let source = concat!(
        "test \"scope\"\n",
        "    for each item in values\n",
        "        set inner to 1\n",
        "    end\n",
        "    set local to 2\n",
        "end\n",
        "test \"other\"\n",
        "    say local\n",
        "end\n",
    );

    let offered = completions(source, 7, 1).expect("the blank line after the test is valid");

    assert!(
        !offered.contains(&"local".to_string()),
        "a test body's own declaration must not reach the next test, got {offered:?}"
    );
    assert!(
        !offered.contains(&"item".to_string()),
        "a loop variable inside the test body closes with its loop, got {offered:?}"
    );
    assert!(
        !offered.contains(&"inner".to_string()),
        "the loop body's declaration also closes with its loop, got {offered:?}"
    );
    assert_eq!(
        goto_definition(source, 8, 9).expect("a position inside the file is valid"),
        None,
        "a name declared in one test must not resolve in a later test"
    );
}

#[test]
fn edge_constant_in_an_inner_scope_is_a_program_binding() {
    // The VM binds a constant to a global whatever block declares it, and the
    // analyzer declares it in the program scope. Navigation must not model it
    // as a local that dies at the loop's `end`.
    let source = concat!(
        "set marker to 1\n",
        "while marker is less than 2\n",
        "    constant limit to 2\n",
        "    say limit\n",
        "end\n",
        "say limit\n",
    );

    let inside = goto_definition(source, 4, 9)
        .expect("a position inside the file is valid")
        .expect("the inner use resolves to the constant declared in the loop");
    assert_eq!(
        (inside.line, inside.column, inside.kind.as_str()),
        (3, 14, "constant"),
        "the loop body's use resolves to the constant it declares"
    );

    let after = goto_definition(source, 6, 5)
        .expect("a position inside the file is valid")
        .expect("a constant declared in a loop is a program binding");
    assert_eq!(
        (after.line, after.column, after.kind.as_str()),
        (3, 14, "constant"),
        "the constant outlives the loop's `end`, as it does in the analyzer and the VM"
    );

    let references: Vec<(usize, usize)> = find_references(source, 4, 9)
        .expect("a position inside the file is valid")
        .into_iter()
        .map(|location| (location.line, location.column))
        .collect();
    assert_eq!(
        references,
        vec![(3, 14), (4, 9), (6, 5)],
        "the declaration and both uses are the one program binding"
    );
}

#[test]
fn edge_constant_after_a_same_named_set_is_its_own_binding() {
    // A constant and an earlier `set` share the program's global slot, but the
    // constant shadows the variable for every use after it. Reusing the
    // variable's binding would make the `constant x` declaration navigate to
    // `set x`, and would join two bindings the runtime keeps apart.
    let source = concat!(
        "set x to 1\n",
        "say x\n",
        "while x is less than 2\n",
        "    constant x to 2\n",
        "    say x\n",
        "end\n",
        "say x\n",
    );

    for (line, column, expected) in [(1, 5, "variable"), (2, 5, "variable"), (3, 7, "variable")] {
        let definition = goto_definition(source, line, column)
            .expect("a position inside the file is valid")
            .expect("the variable use resolves to its declaration");
        assert_eq!(
            (definition.line, definition.column, definition.kind.as_str()),
            (1, 5, expected),
            "line {line} is before the constant, so it names the variable"
        );
    }

    for (line, column) in [(4, 14), (5, 9), (7, 5)] {
        let definition = goto_definition(source, line, column)
            .expect("a position inside the file is valid")
            .expect("the constant declaration and its uses resolve to the constant");
        assert_eq!(
            (definition.line, definition.column, definition.kind.as_str()),
            (4, 14, "constant"),
            "line {line} names the constant, not the earlier variable"
        );
    }

    let variable_references: Vec<(usize, usize)> = find_references(source, 1, 5)
        .expect("a position inside the file is valid")
        .into_iter()
        .map(|location| (location.line, location.column))
        .collect();
    assert_eq!(
        variable_references,
        vec![(1, 5), (2, 5), (3, 7)],
        "the variable's references stop at the constant declaration"
    );

    let constant_references: Vec<(usize, usize)> = find_references(source, 4, 14)
        .expect("a position inside the file is valid")
        .into_iter()
        .map(|location| (location.line, location.column))
        .collect();
    assert_eq!(
        constant_references,
        vec![(4, 14), (5, 9), (7, 5)],
        "the constant's references do not join the earlier `set`"
    );
}

#[test]
fn edge_set_after_a_same_named_constant_is_its_own_binding() {
    // A `set` cannot write through a constant, so it starts a new variable
    // binding in the runtime frame. Reusing the constant would make the `set`
    // declaration navigate to `constant x`, joining an immutable declaration
    // with the attempted assignment.
    let source = concat!(
        "constant x to 1\n",
        "say x\n",
        "while x is less than 2\n",
        "    set x to 2\n",
        "    say x\n",
        "end\n",
        "say x\n",
    );

    for (line, column) in [(1, 10), (2, 5), (3, 7)] {
        let definition = goto_definition(source, line, column)
            .expect("a position inside the file is valid")
            .expect("the use before the set resolves to the constant");
        assert_eq!(
            (definition.line, definition.column, definition.kind.as_str()),
            (1, 10, "constant"),
            "line {line} is before the set, so it names the constant"
        );
    }

    for (line, column) in [(4, 9), (5, 9), (7, 5)] {
        let definition = goto_definition(source, line, column)
            .expect("a position inside the file is valid")
            .expect("the set declaration and its uses resolve to the variable");
        assert_eq!(
            (definition.line, definition.column, definition.kind.as_str()),
            (4, 9, "variable"),
            "line {line} names the variable the set declares, not the earlier constant"
        );
    }

    let constant_references: Vec<(usize, usize)> = find_references(source, 1, 10)
        .expect("a position inside the file is valid")
        .into_iter()
        .map(|location| (location.line, location.column))
        .collect();
    assert_eq!(
        constant_references,
        vec![(1, 10), (2, 5), (3, 7)],
        "the constant's references stop at the set declaration"
    );

    let variable_references: Vec<(usize, usize)> = find_references(source, 4, 9)
        .expect("a position inside the file is valid")
        .into_iter()
        .map(|location| (location.line, location.column))
        .collect();
    assert_eq!(
        variable_references,
        vec![(4, 9), (5, 9), (7, 5)],
        "the variable's references do not join the earlier constant"
    );
}

#[test]
fn edge_finally_body_leaves_the_catch_scope() {
    let source = concat!(
        "try\n",
        "    set marker to 1\n",
        "catch problem\n",
        "    say problem\n",
        "finally\n",
        "    say problem\n",
        "    set cleaned to 2\n",
        "end\n",
        "say cleaned\n",
        "say problem\n",
    );

    let caught = goto_definition(source, 4, 9)
        .expect("a position inside the file is valid")
        .expect("the catch body still resolves its own binder");
    assert_eq!(
        (caught.line, caught.column, caught.kind.as_str()),
        (3, 7, "catch"),
        "the catch body's use must resolve to its binder"
    );

    assert_eq!(
        goto_definition(source, 6, 9).expect("a position inside the file is valid"),
        None,
        "the catch binder must not be visible in the `finally` body"
    );

    let cleaned = goto_definition(source, 9, 5)
        .expect("a position inside the file is valid")
        .expect("a `finally` declaration lands in the enclosing scope");
    assert_eq!(
        (cleaned.line, cleaned.column, cleaned.kind.as_str()),
        (7, 9, "variable"),
        "the `finally` body must not be analyzed inside the catch scope"
    );

    assert_eq!(
        goto_definition(source, 10, 5).expect("a position inside the file is valid"),
        None,
        "the catch binder must not leak past the try's `end`"
    );
}

#[test]
fn edge_first_set_inside_a_function_is_a_program_binding() {
    let source = concat!(
        "to show()\n",
        "    unless no\n",
        "        set local to 1\n",
        "    end\n",
        "    say local\n",
        "end\n",
        "say local\n",
    );

    let inside = goto_definition(source, 5, 9)
        .expect("a position inside the file is valid")
        .expect("the function body resolves the name its unless body set");
    assert_eq!(
        (inside.line, inside.column, inside.kind.as_str()),
        (3, 13, "variable"),
        "the body's use resolves to the first assignment written in the unless"
    );

    let after = goto_definition(source, 7, 5)
        .expect("a position inside the file is valid")
        .expect("a first assignment in a function is a program binding");
    assert_eq!(
        (after.line, after.column, after.kind.as_str()),
        (3, 13, "variable"),
        "the VM writes the first assignment to the program scope, so the function's `end` keeps it"
    );
}

#[test]
fn edge_first_set_inside_a_loop_is_a_program_binding() {
    let source = concat!(
        "for each item in items\n",
        "    set total to item\n",
        "end\n",
        "say total\n",
    );

    let inside = goto_definition(source, 2, 9)
        .expect("a position inside the file is valid")
        .expect("the loop body resolves the name it sets");
    assert_eq!(
        (inside.line, inside.column, inside.kind.as_str()),
        (2, 9, "variable"),
        "the declaration is the loop body's first assignment"
    );

    let after = goto_definition(source, 4, 5)
        .expect("a position inside the file is valid")
        .expect("a first assignment in a loop is a program binding");
    assert_eq!(
        (after.line, after.column, after.kind.as_str()),
        (2, 9, "variable"),
        "the loop's `end` must not hide the program binding the assignment created"
    );
}

#[test]
fn edge_navigation_rejects_a_position_past_an_unless_body() {
    let source = "unless no then\n    set local to 1\nend\n";

    assert!(
        goto_definition(source, 99, 1).is_err(),
        "a line past the unless body is a clean error"
    );
    assert!(
        find_references(source, 99, 1).is_err(),
        "a line past the unless body is a clean error"
    );
    assert!(
        completions(source, 99, 1).is_err(),
        "a line past the unless body is a clean error"
    );
}

#[test]
fn edge_try_catch_scope_does_not_leak_into_the_enclosing_scope() {
    let source = concat!(
        "set marker to 1\n",
        "try\n",
        "    say marker\n",
        "catch problem\n",
        "    say problem\n",
        "end\n",
        "say marker\n",
        "say problem\n",
    );

    let binder = goto_definition(source, 5, 9)
        .expect("a position inside the file is valid")
        .expect("the caught name is in scope inside the catch body");
    assert_eq!(
        (binder.line, binder.column, binder.kind.as_str()),
        (4, 7, "catch"),
        "the catch body's use must resolve to its binder"
    );

    let after = goto_definition(source, 7, 5)
        .expect("a position inside the file is valid")
        .expect("the enclosing binding is still in scope after the try closes");
    assert_eq!(
        (after.line, after.column, after.kind.as_str()),
        (1, 5, "variable"),
        "the try `end` must close only the catch scope"
    );
    assert_eq!(
        goto_definition(source, 8, 5).expect("a position inside the file is valid"),
        None,
        "a catch binder must not leak past the try's `end`"
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

    assert_eq!(
        goto_definition(source, 1, 19).expect("a position inside the file is valid"),
        None,
        "the `of list` type must not declare `list` or resolve it to the module builtin"
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
fn edge_range_with_an_lru_cache_lower_bound_does_not_push_a_function_frame() {
    let source = concat!(
        "set lru_cache to 3\n",
        "set limit to 10\n",
        "for each i from lru_cache to limit\n",
        "    say i\n",
        "end\n",
        "say i\n",
    );

    let inside = goto_definition(source, 4, 9)
        .expect("a position inside the file is valid")
        .expect("the loop variable is in scope inside the range loop");
    assert_eq!(
        (inside.line, inside.column, inside.kind.as_str()),
        (3, 10, "loop"),
        "an ordinary `lru_cache` variable before a range's `to` is not a decorator"
    );

    assert_eq!(
        goto_definition(source, 6, 5).expect("a position inside the file is valid"),
        None,
        "the loop variable must not leak past the range loop's `end`"
    );
}

#[test]
fn edge_parameterless_function_literal_end_keeps_the_enclosing_scope() {
    let source = concat!(
        "to outer(limit)\n",
        "    set f to to give back limit end\n",
        "    say limit\n",
        "end\n",
        "say limit\n",
    );

    let inside = goto_definition(source, 3, 9)
        .expect("a position inside the file is valid")
        .expect("the parameter is still in scope after the literal's `end`");
    assert_eq!(
        (inside.line, inside.column, inside.kind.as_str()),
        (1, 10, "parameter"),
        "a parameterless literal's `end` must close the literal, not the function around it"
    );

    assert_eq!(
        goto_definition(source, 5, 5).expect("a position inside the file is valid"),
        None,
        "the parameter must not leak past the outer function's own `end`"
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

#[test]
fn edge_goto_definition_on_a_declaration_returns_its_own_span() {
    let source = "set greeting to \"hi\"\nsay greeting\n";

    let found = goto_definition(source, 1, 5)
        .expect("a position inside the file is valid")
        .expect("a declaration names its own binding");

    assert_eq!(
        (found.line, found.column, found.kind.as_str()),
        (1, 5, "variable"),
        "goto on the `set` name must answer the declaration itself"
    );
}

#[test]
fn edge_completions_on_an_empty_line_offer_keywords_builtins_and_scope() {
    let source = "set alpha to 1\n\n";

    let offered = completions(source, 2, 1).expect("the empty line is a valid position");

    for expected in ["alpha", "set", "abs"] {
        assert!(
            offered.contains(&expected.to_string()),
            "completion on an empty line must offer {expected:?}, got {offered:?}"
        );
    }
}

#[test]
fn edge_completions_after_a_function_end_exclude_its_parameters() {
    let source = concat!("to show(param)\n", "    say param\n", "end\n", "say pa\n");
    let end_column = source
        .lines()
        .nth(3)
        .map(|line| line.chars().count() + 1)
        .unwrap();

    let offered = completions(source, 4, end_column).expect("the end of the line is valid");

    assert!(
        !offered.contains(&"param".to_string()),
        "a parameter must not leak past its function's `end`, got {offered:?}"
    );
    assert!(
        offered.contains(&"show".to_string()),
        "the function name itself remains in the enclosing scope, got {offered:?}"
    );
}

#[test]
fn edge_navigation_rejects_an_enormous_line_number() {
    let source = "set x to 1\n";

    assert!(
        goto_definition(source, usize::MAX, 1).is_err(),
        "an out-of-range line must be a clean error, not a panic"
    );
    assert!(
        find_references(source, usize::MAX, 1).is_err(),
        "an out-of-range line must be a clean error, not a panic"
    );
    assert!(
        completions(source, usize::MAX, 1).is_err(),
        "an out-of-range line must be a clean error, not a panic"
    );
}

#[test]
fn edge_assignment_to_inside_an_inline_block_keeps_the_function_scope_balanced() {
    let source = concat!(
        "to f(limit)\n",
        "    if limit is 1 then set flag to yes end\n",
        "    say limit\n",
        "end\n",
        "say limit\n",
    );

    let inside = goto_definition(source, 3, 9)
        .expect("a position inside the file is valid")
        .expect("the parameter is still in scope after the inline block closes");
    assert_eq!(
        (inside.line, inside.column, inside.kind.as_str()),
        (1, 6, "parameter"),
        "the assignment's `to` must not open a literal frame that steals the block `end`"
    );

    assert_eq!(
        goto_definition(source, 5, 5).expect("a position inside the file is valid"),
        None,
        "the parameter must not leak past the function's own `end`"
    );
}

#[test]
fn edge_object_member_access_navigates_a_forward_reference() {
    let source = concat!("say p.name\n", "object p\n", "    has name\n", "end\n");

    let found = goto_definition(source, 1, 7)
        .expect("a position inside the file is valid")
        .expect("a member access resolves through a receiver declared below it");
    assert_eq!(
        (found.line, found.column, found.kind.as_str()),
        (3, 9, "variable"),
        "`p.name` must resolve to the field declared by `has name`"
    );

    let references: Vec<(usize, usize)> = find_references(source, 3, 9)
        .expect("a position inside the file is valid")
        .into_iter()
        .map(|location| (location.line, location.column))
        .collect();
    assert_eq!(
        references,
        vec![(1, 7), (3, 9)],
        "the field's references must include its forward use"
    );
}

#[test]
fn edge_object_member_access_navigates_this_and_ordinary_reference() {
    let source = concat!(
        "object Person\n",
        "    has name\n",
        "    to can greet()\n",
        "        give back this.name\n",
        "    end\n",
        "end\n",
        "say Person.name\n",
    );

    for (line, column) in [(4, 24), (7, 12)] {
        let found = goto_definition(source, line, column)
            .expect("a position inside the file is valid")
            .expect("a member access through `this` or an object name resolves");
        assert_eq!(
            (found.line, found.column, found.kind.as_str()),
            (2, 9, "variable"),
            "`this.name` and `Person.name` must both resolve to the `has name` field"
        );
    }

    let references: Vec<(usize, usize)> = find_references(source, 2, 9)
        .expect("a position inside the file is valid")
        .into_iter()
        .map(|location| (location.line, location.column))
        .collect();
    assert_eq!(
        references,
        vec![(2, 9), (4, 24), (7, 12)],
        "both member uses must belong to the field's references"
    );
}

#[test]
fn edge_builtin_references_exclude_member_and_type_names() {
    let member = "set holder to 1\nsay holder.abs\nsay abs(-1)\n";
    let member_references: Vec<(usize, usize)> = find_references(member, 3, 5)
        .expect("a position inside the file is valid")
        .into_iter()
        .map(|location| (location.line, location.column))
        .collect();
    assert_eq!(
        member_references,
        vec![(3, 5)],
        "`holder.abs` is a member name, not a use of the `abs` builtin"
    );
    assert!(
        find_references(member, 2, 12)
            .expect("a position inside the file is valid")
            .is_empty(),
        "querying the member name itself must not answer with the plain `abs` use"
    );

    let typed = "say abs\nto total(items of abs)\n    give back items\nend\n";
    let type_references: Vec<(usize, usize)> = find_references(typed, 1, 5)
        .expect("a position inside the file is valid")
        .into_iter()
        .map(|location| (location.line, location.column))
        .collect();
    assert_eq!(
        type_references,
        vec![(1, 5)],
        "the `abs` in `items of abs` is a type name, not a use of the `abs` builtin"
    );
    assert!(
        find_references(typed, 2, 19)
            .expect("a position inside the file is valid")
            .is_empty(),
        "querying the type name itself must not answer with the plain `abs` use"
    );
    assert_eq!(
        goto_definition(typed, 2, 19).expect("a position inside the file is valid"),
        None,
        "a type name spelled like a builtin must not resolve to the builtin registry"
    );

    let builtin = goto_definition(typed, 1, 5)
        .expect("a position inside the file is valid")
        .expect("the plain `abs` is still the builtin");
    assert_eq!(
        (builtin.kind.as_str(), builtin.name.as_str()),
        ("builtin", "abs"),
        "a type name spelled like a builtin must not hide the real builtin use"
    );
}

#[test]
fn edge_goto_definition_on_a_member_name_does_not_resolve_to_a_builtin() {
    let source = "set holder to 1\nsay holder.abs\nsay abs(-1)\n";

    assert_eq!(
        goto_definition(source, 2, 12).expect("a position inside the file is valid"),
        None,
        "`holder.abs` is a member name, not a use of the `abs` builtin"
    );

    let builtin = goto_definition(source, 3, 5)
        .expect("a position inside the file is valid")
        .expect("the plain `abs` call is still the builtin");
    assert_eq!(
        (builtin.kind.as_str(), builtin.name.as_str()),
        ("builtin", "abs"),
        "the member guard must not hide a real use of the builtin"
    );
}

#[test]
fn edge_builtin_use_before_a_later_set_stays_the_builtin() {
    let source = "say abs\nset abs to 1\nsay abs\n";

    let first = goto_definition(source, 1, 5)
        .expect("a position inside the file is valid")
        .expect("the use before the later `set` is still the builtin");
    assert_eq!(
        (first.kind.as_str(), first.name.as_str()),
        ("builtin", "abs"),
        "a later first `set` must not rebind an earlier builtin use"
    );

    let first_references: Vec<(usize, usize)> = find_references(source, 1, 5)
        .expect("a position inside the file is valid")
        .into_iter()
        .map(|location| (location.line, location.column))
        .collect();
    assert_eq!(
        first_references,
        vec![(1, 5)],
        "the builtin reference set must not include the later variable or its uses"
    );

    let declaration = goto_definition(source, 2, 5)
        .expect("a position inside the file is valid")
        .expect("the `set` declares its own binding");
    assert_eq!(
        (
            declaration.line,
            declaration.column,
            declaration.kind.as_str()
        ),
        (2, 5, "variable"),
        "the later `set abs` still declares a variable"
    );

    let variable_references: Vec<(usize, usize)> = find_references(source, 2, 5)
        .expect("a position inside the file is valid")
        .into_iter()
        .map(|location| (location.line, location.column))
        .collect();
    assert_eq!(
        variable_references,
        vec![(2, 5), (3, 5)],
        "the variable's references start at its declaration, not at the earlier builtin use"
    );
}

#[test]
fn edge_navigation_rejects_a_position_past_a_builtin_before_a_later_set() {
    let source = "say abs\nset abs to 1\n";

    assert!(
        goto_definition(source, 1, 99).is_err(),
        "goto past the line is a clean error"
    );
    assert!(
        find_references(source, 1, 99).is_err(),
        "references past the line is a clean error"
    );
    assert!(
        completions(source, 1, 99).is_err(),
        "completion past the line is a clean error"
    );
}

#[test]
fn edge_navigation_handles_cr_and_crlf_line_endings() {
    for (label, newline) in [("lone CR", "\r"), ("CRLF", "\r\n")] {
        let source = format!("set value to 1{newline}say value // note{newline}say value{newline}");

        let found = goto_definition(&source, 2, 5)
            .unwrap_or_else(|error| panic!("{label}: goto on line 2 failed: {error}"))
            .unwrap_or_else(|| panic!("{label}: the `value` use on line 2 must resolve"));
        assert_eq!(
            (found.line, found.column, found.kind.as_str()),
            (1, 5, "variable"),
            "{label}: goto must map lines after the first"
        );

        let references: Vec<(usize, usize)> = find_references(&source, 3, 5)
            .unwrap_or_else(|error| panic!("{label}: references on line 3 failed: {error}"))
            .into_iter()
            .map(|location| (location.line, location.column))
            .collect();
        assert_eq!(
            references,
            vec![(1, 5), (2, 5), (3, 5)],
            "{label}: every use of `value` must be found"
        );

        assert!(
            completions(&source, 2, 15)
                .unwrap_or_else(|error| panic!(
                    "{label}: completion in the comment failed: {error}"
                ))
                .is_empty(),
            "{label}: a caret inside `// note` must offer nothing"
        );

        let offered = completions(&source, 3, 10)
            .unwrap_or_else(|error| panic!("{label}: completion on line 3 failed: {error}"));
        assert!(
            offered.contains(&"value".to_string()),
            "{label}: a comment must end at the line break, not run to EOF, got {offered:?}"
        );
    }
}

#[test]
fn edge_completions_at_the_opening_quote_and_comment_marker_are_empty() {
    let source = "set x to \"hi\"\n// note\n";

    assert!(
        completions(source, 1, 10)
            .expect("the opening quote is a valid position")
            .is_empty(),
        "a caret on the opening quote is inside the text, not code"
    );
    assert!(
        completions(source, 2, 1)
            .expect("the comment marker is a valid position")
            .is_empty(),
        "a caret on the first `/` of `//` is inside the comment, not code"
    );
}

#[test]
fn edge_malformed_source_error_carries_the_lexer_span() {
    let source = "say \"unterminated";

    let messages = [
        goto_definition(source, 1, 1).expect_err("an unterminated string is a lexer error"),
        find_references(source, 1, 1).expect_err("an unterminated string is a lexer error"),
        completions(source, 1, 1).expect_err("an unterminated string is a lexer error"),
    ];

    for message in messages {
        assert!(
            message.contains("1:5"),
            "the error must carry the lexer span, got {message:?}"
        );
        assert!(
            message.contains("Unterminated string"),
            "the error must keep the lexer message, got {message:?}"
        );
    }
}

#[test]
fn edge_goto_definition_on_object_method_call_resolves_to_the_method_declaration() {
    let source = concat!(
        "object Greeter\n",
        "    has name default \"world\"\n",
        "    to can greet(greeting)\n",
        "        give back greeting + this.name\n",
        "    end\n",
        "end\n",
        "say Greeter.greet(\"hi\")\n",
    );

    let found = goto_definition(source, 7, 13)
        .expect("a position inside the file is valid")
        .expect("an object method call must resolve to its declaration");
    assert_eq!(
        (found.line, found.column, found.kind.as_str()),
        (3, 12, "function"),
        "`Greeter.greet` must resolve to the `to can greet` declaration"
    );

    let references: Vec<(usize, usize)> = find_references(source, 3, 12)
        .expect("a position inside the file is valid")
        .into_iter()
        .map(|location| (location.line, location.column))
        .collect();
    assert_eq!(
        references,
        vec![(3, 12), (7, 13)],
        "the method's references must include its call site"
    );
}

#[test]
fn edge_goto_definition_on_a_this_method_call_resolves_to_the_method_declaration() {
    let source = concat!(
        "object Greeter\n",
        "    to can greet()\n",
        "        give back this.greet()\n",
        "    end\n",
        "end\n",
    );

    let found = goto_definition(source, 3, 24)
        .expect("a position inside the file is valid")
        .expect("a method call through `this` must resolve to its declaration");
    assert_eq!(
        (found.line, found.column, found.kind.as_str()),
        (2, 12, "function"),
        "`this.greet()` must resolve to the `to can greet` declaration"
    );
}

#[test]
fn edge_object_method_and_field_of_the_same_name_stay_distinct() {
    let source = concat!(
        "object Row\n",
        "    has name default \"field\"\n",
        "    to can name()\n",
        "        give back \"method\"\n",
        "    end\n",
        "end\n",
        "say Row.name\n",
        "say Row.name()\n",
    );

    let field = goto_definition(source, 7, 9)
        .expect("a position inside the file is valid")
        .expect("a bare member access resolves to the field");
    assert_eq!(
        (field.line, field.column, field.kind.as_str()),
        (2, 9, "variable"),
        "`Row.name` must resolve to `has name`, not the method of the same name"
    );

    let method = goto_definition(source, 8, 9)
        .expect("a position inside the file is valid")
        .expect("a member call resolves to the method");
    assert_eq!(
        (method.line, method.column, method.kind.as_str()),
        (3, 12, "function"),
        "`Row.name()` must resolve to `to can name`, not the field of the same name"
    );

    let field_references: Vec<(usize, usize)> = find_references(source, 2, 9)
        .expect("a position inside the file is valid")
        .into_iter()
        .map(|location| (location.line, location.column))
        .collect();
    assert_eq!(
        field_references,
        vec![(2, 9), (7, 9)],
        "the field's references must not include the method call"
    );

    let method_references: Vec<(usize, usize)> = find_references(source, 3, 12)
        .expect("a position inside the file is valid")
        .into_iter()
        .map(|location| (location.line, location.column))
        .collect();
    assert_eq!(
        method_references,
        vec![(3, 12), (8, 9)],
        "the method's references must not include the field access"
    );
}

#[test]
fn edge_object_field_declared_after_its_method_still_wins_a_member_read() {
    let source = concat!(
        "object Row\n",
        "    to can name()\n",
        "        give back this.name\n",
        "    end\n",
        "    has name default \"field\"\n",
        "end\n",
        "say Row.name\n",
        "say Row.name()\n",
    );

    let member = goto_definition(source, 3, 24)
        .expect("a position inside the file is valid")
        .expect("a member read resolves to the field declared below it");
    assert_eq!(
        (member.line, member.column, member.kind.as_str()),
        (5, 9, "variable"),
        "`this.name` must prefer the field even when the method is declared first"
    );

    let field = goto_definition(source, 7, 9)
        .expect("a position inside the file is valid")
        .expect("a bare member access resolves to the field");
    assert_eq!(
        (field.line, field.column, field.kind.as_str()),
        (5, 9, "variable"),
        "`Row.name` must resolve to `has name`, not the method of the same name"
    );

    let method = goto_definition(source, 8, 9)
        .expect("a position inside the file is valid")
        .expect("a member call resolves to the method");
    assert_eq!(
        (method.line, method.column, method.kind.as_str()),
        (2, 12, "function"),
        "`Row.name()` must resolve to `to can name`, not the field of the same name"
    );

    let field_references: Vec<(usize, usize)> = find_references(source, 5, 9)
        .expect("a position inside the file is valid")
        .into_iter()
        .map(|location| (location.line, location.column))
        .collect();
    assert_eq!(
        field_references,
        vec![(3, 24), (5, 9), (7, 9)],
        "the field's references must include the `this` read and the object member access"
    );

    let method_references: Vec<(usize, usize)> = find_references(source, 2, 12)
        .expect("a position inside the file is valid")
        .into_iter()
        .map(|location| (location.line, location.column))
        .collect();
    assert_eq!(
        method_references,
        vec![(2, 12), (8, 9)],
        "the method's references must not include the field's `this` read"
    );
}

#[test]
fn edge_object_field_is_not_a_bare_lexical_name() {
    let source = concat!(
        "object Row\n",
        "    has name default \"field\"\n",
        "    to can label()\n",
        "        give back name\n",
        "    end\n",
        "end\n",
    );

    assert_eq!(
        goto_definition(source, 4, 19).expect("a position inside the file is valid"),
        None,
        "the analyzer rejects a bare field read, so navigation must not bind it"
    );
    assert!(
        find_references(source, 4, 19)
            .expect("a position inside the file is valid")
            .is_empty(),
        "a bare field read has no binding, so it has no references"
    );
}

#[test]
fn edge_method_name_is_not_a_bare_lexical_name() {
    let source = concat!(
        "object Row\n",
        "    to can name()\n",
        "        give back name\n",
        "    end\n",
        "end\n",
    );

    assert_eq!(
        goto_definition(source, 3, 19).expect("a position inside the file is valid"),
        None,
        "the analyzer rejects a bare method read, so navigation must not bind it"
    );
    assert!(
        find_references(source, 3, 19)
            .expect("a position inside the file is valid")
            .is_empty(),
        "a bare method read has no binding, so it has no references"
    );
}

#[test]
fn edge_bare_method_call_prefers_the_method_over_a_same_named_field() {
    let source = concat!(
        "object Row\n",
        "    has b default 1\n",
        "    to can b()\n",
        "        give back 2\n",
        "    end\n",
        "    to can a()\n",
        "        give back b()\n",
        "    end\n",
        "end\n",
    );

    let method = goto_definition(source, 7, 19)
        .expect("a position inside the file is valid")
        .expect("a bare call must resolve to the method, not the field");
    assert_eq!(
        (method.line, method.column, method.kind.as_str()),
        (3, 12, "function"),
        "`b()` must resolve to `to can b`, not the field of the same name"
    );

    let method_references: Vec<(usize, usize)> = find_references(source, 3, 12)
        .expect("a position inside the file is valid")
        .into_iter()
        .map(|location| (location.line, location.column))
        .collect();
    assert_eq!(
        method_references,
        vec![(3, 12), (7, 19)],
        "the method's references must include the bare call"
    );

    let field_references: Vec<(usize, usize)> = find_references(source, 2, 9)
        .expect("a position inside the file is valid")
        .into_iter()
        .map(|location| (location.line, location.column))
        .collect();
    assert_eq!(
        field_references,
        vec![(2, 9)],
        "the field's references must not include the method call"
    );
}

#[test]
fn edge_navigation_rejects_a_position_past_the_file() {
    let source = "set x to 1\n";

    assert!(
        goto_definition(source, 99, 1).is_err(),
        "a line past the file must be a clean goto error"
    );
    assert!(
        find_references(source, 99, 1).is_err(),
        "a line past the file must be a clean references error"
    );
    assert!(
        completions(source, 99, 1).is_err(),
        "a line past the file must be a clean completion error"
    );
}

#[test]
fn edge_find_references_on_an_unknown_name_returns_no_binding() {
    let source = "say frobnicate\nsay frobnicate\n";

    assert_eq!(
        goto_definition(source, 1, 5).expect("a position inside the file is valid"),
        None,
        "an unknown word has no definition to jump to"
    );

    let references = find_references(source, 1, 5).expect("a position inside the file is valid");
    assert!(
        references.is_empty(),
        "an unknown word is not a binding, so it has no references, got {references:?}"
    );
}

#[test]
fn edge_find_references_on_an_unknown_name_beside_a_real_binding_still_answers_the_binding() {
    let source = "set value to 1\nsay frobnicate\nsay value\n";

    let references: Vec<(usize, usize)> = find_references(source, 3, 5)
        .expect("a position inside the file is valid")
        .into_iter()
        .map(|location| (location.line, location.column))
        .collect();
    assert_eq!(
        references,
        vec![(1, 5), (3, 5)],
        "an unknown word must not join a real binding's references"
    );
}

#[test]
fn edge_find_references_rejects_a_zero_column() {
    let source = "set value to 1\n";

    assert!(
        find_references(source, 1, 0).is_err(),
        "column 0 is not a source position and must be a clean error"
    );
}

#[test]
fn edge_references_in_sibling_functions_do_not_cross() {
    let source = concat!(
        "to first(x)\n",
        "    say x\n",
        "end\n",
        "to second(x)\n",
        "    say x\n",
        "end\n",
    );

    let first: Vec<(usize, usize)> = find_references(source, 1, 10)
        .expect("a position inside the file is valid")
        .into_iter()
        .map(|location| (location.line, location.column))
        .collect();
    assert_eq!(
        first,
        vec![(1, 10), (2, 9)],
        "the first function's parameter must not pick up the sibling's use"
    );

    let second: Vec<(usize, usize)> = find_references(source, 4, 11)
        .expect("a position inside the file is valid")
        .into_iter()
        .map(|location| (location.line, location.column))
        .collect();
    assert_eq!(
        second,
        vec![(4, 11), (5, 9)],
        "the second function's parameter must not pick up the sibling's use"
    );
}

#[test]
fn edge_completion_before_a_later_declaration_does_not_offer_it() {
    let source = "say later\nset later to 1\n";

    let offered = completions(source, 1, 1).expect("a position inside the file is valid");
    assert!(
        !offered.iter().any(|name| name == "later"),
        "a name declared below the caret is not in scope yet, got {offered:?}"
    );
    assert!(
        offered.iter().any(|name| name == "say"),
        "the keyword set is still offered at the top of the file"
    );
}

#[test]
fn edge_goto_definition_on_whitespace_and_text_returns_none() {
    let source = "say x\nsay \"x\"\n";

    assert_eq!(
        goto_definition(source, 1, 4).expect("a position inside the file is valid"),
        None,
        "whitespace names no binding"
    );
    assert_eq!(
        goto_definition(source, 2, 6).expect("a position inside the file is valid"),
        None,
        "the body of a text literal names no binding"
    );
}

#[test]
fn edge_navigation_rejects_line_zero_for_every_capability() {
    let source = "set x to 1\n";

    assert!(
        goto_definition(source, 0, 1).is_err(),
        "line 0 is not a source position for goto-definition"
    );
    assert!(
        find_references(source, 0, 1).is_err(),
        "line 0 is not a source position for find-references"
    );
    assert!(
        completions(source, 0, 1).is_err(),
        "line 0 is not a source position for completion"
    );
}

#[test]
fn edge_syntax_names_do_not_answer_as_builtin_references() {
    let source = "try\n    say 1\ncatch error of map\n    say 1\nend\nsay map([1, 2])\n";

    assert_eq!(
        goto_definition(source, 3, 14).expect("a position inside the file is valid"),
        None,
        "a catch failure kind spelled like a builtin is syntax, not the builtin"
    );
    assert!(
        find_references(source, 3, 14)
            .expect("a position inside the file is valid")
            .is_empty(),
        "a catch failure kind must not answer with the plain builtin's references"
    );

    let references: Vec<(usize, usize)> = find_references(source, 6, 5)
        .expect("a position inside the file is valid")
        .into_iter()
        .map(|location| (location.line, location.column))
        .collect();
    assert_eq!(
        references,
        vec![(6, 5)],
        "the plain builtin use must not pick up the contextual catch kind"
    );
}

#[test]
fn edge_goto_definition_resolves_an_inherited_field() {
    let source = concat!(
        "object Person\n",
        "    has age default 1\n",
        "end\n",
        "object Employee extends Person\n",
        "end\n",
        "say Employee.age\n",
    );

    let found = goto_definition(source, 6, 14)
        .expect("a position inside the file is valid")
        .expect("an inherited field resolves through the parent object");

    assert_eq!(
        (found.line, found.column, found.kind.as_str()),
        (2, 9, "variable"),
        "the field is declared on Person, not copied onto Employee"
    );

    let references: Vec<(usize, usize)> = find_references(source, 6, 14)
        .expect("a position inside the file is valid")
        .into_iter()
        .map(|location| (location.line, location.column))
        .collect();
    assert_eq!(
        references,
        vec![(2, 9), (6, 14)],
        "the declaration and the inherited read are one binding"
    );
}

#[test]
fn edge_find_references_on_an_inherited_method_spans_the_parent_declaration() {
    let source = concat!(
        "object Person\n",
        "    to can greet(name)\n",
        "        give back name\n",
        "    end\n",
        "end\n",
        "object Employee extends Person\n",
        "end\n",
        "say Employee.greet(\"Ada\")\n",
    );

    let found = goto_definition(source, 8, 14)
        .expect("a position inside the file is valid")
        .expect("an inherited method resolves through the parent object");

    assert_eq!(
        (found.line, found.column, found.kind.as_str()),
        (2, 12, "function"),
        "the method declaration stays on Person"
    );

    let references: Vec<(usize, usize)> = find_references(source, 8, 14)
        .expect("a position inside the file is valid")
        .into_iter()
        .map(|location| (location.line, location.column))
        .collect();
    assert_eq!(
        references,
        vec![(2, 12), (8, 14)],
        "the inherited call joins the method declaration"
    );
}

#[test]
fn edge_inherited_member_shadowing_prefers_the_child_declaration() {
    let source = concat!(
        "object Person\n",
        "    has name default \"parent\"\n",
        "end\n",
        "object Employee extends Person\n",
        "    has name default \"child\"\n",
        "end\n",
        "say Employee.name\n",
    );

    let child = goto_definition(source, 7, 14)
        .expect("a position inside the file is valid")
        .expect("the child field shadows the parent field");
    assert_eq!(
        (child.line, child.column),
        (5, 9),
        "the nearest declaration wins, as the runtime's copied table does"
    );

    let child_references: Vec<(usize, usize)> = find_references(source, 7, 14)
        .expect("a position inside the file is valid")
        .into_iter()
        .map(|location| (location.line, location.column))
        .collect();
    assert_eq!(
        child_references,
        vec![(5, 9), (7, 14)],
        "the shadowed child field keeps its own reference set"
    );

    let parent_references: Vec<(usize, usize)> = find_references(source, 2, 9)
        .expect("a position inside the file is valid")
        .into_iter()
        .map(|location| (location.line, location.column))
        .collect();
    assert_eq!(
        parent_references,
        vec![(2, 9)],
        "the parent field must not absorb the child's declaration or read"
    );
}

#[test]
fn edge_this_member_resolves_through_the_parent_chain() {
    let source = concat!(
        "object Person\n",
        "    has age default 1\n",
        "end\n",
        "object Employee extends Person\n",
        "    to can describe()\n",
        "        give back this.age\n",
        "    end\n",
        "end\n",
    );

    let found = goto_definition(source, 6, 24)
        .expect("a position inside the file is valid")
        .expect("`this.age` resolves the inherited field through the child");

    assert_eq!(
        (found.line, found.column),
        (2, 9),
        "the member read resolves to the ancestor that declared the field"
    );

    let references: Vec<(usize, usize)> = find_references(source, 6, 24)
        .expect("a position inside the file is valid")
        .into_iter()
        .map(|location| (location.line, location.column))
        .collect();
    assert_eq!(
        references,
        vec![(2, 9), (6, 24)],
        "`this.age` joins the field declaration it reads"
    );
}

#[test]
fn edge_extends_does_not_bind_a_parent_declared_after_the_child() {
    let source = concat!(
        "object Employee extends Person\n",
        "end\n",
        "object Person\n",
        "    has age default 1\n",
        "end\n",
        "say Employee.age\n",
    );

    assert_eq!(
        goto_definition(source, 6, 14).expect("a position inside the file is valid"),
        None,
        "the interpreter refuses a parent declared after its child, so navigation must not inherit from it"
    );
    assert!(
        find_references(source, 6, 14)
            .expect("a position inside the file is valid")
            .is_empty(),
        "a forward `extends` is not a parent binding, so the member has no references"
    );
}

#[test]
fn edge_goto_definition_resolves_a_grandparent_field() {
    let source = concat!(
        "object GrandParent\n",
        "    has id default 7\n",
        "end\n",
        "object Parent extends GrandParent\n",
        "end\n",
        "object Child extends Parent\n",
        "end\n",
        "say Child.id\n",
    );

    let found = goto_definition(source, 8, 11)
        .expect("a position inside the file is valid")
        .expect("a field two levels up resolves through the whole parent chain");

    assert_eq!(
        (found.line, found.column, found.kind.as_str()),
        (2, 9, "variable"),
        "the field stays on GrandParent rather than moving to Parent or Child"
    );

    let references: Vec<(usize, usize)> = find_references(source, 8, 11)
        .expect("a position inside the file is valid")
        .into_iter()
        .map(|location| (location.line, location.column))
        .collect();
    assert_eq!(
        references,
        vec![(2, 9), (8, 11)],
        "a grandchild read joins the grandparent declaration"
    );
}

#[test]
fn edge_inherited_member_navigation_rejects_a_column_past_the_line() {
    let source = concat!(
        "object Person\n",
        "    has age default 1\n",
        "end\n",
        "object Employee extends Person\n",
        "end\n",
        "say Employee.age\n",
    );

    assert!(
        goto_definition(source, 6, 99).is_err(),
        "a column past the line is not a source position for goto-definition"
    );
    assert!(
        find_references(source, 6, 99).is_err(),
        "a column past the line is not a source position for find-references"
    );
    assert!(
        completions(source, 6, 99).is_err(),
        "a column past the line is not a source position for completion"
    );
}

#[test]
fn edge_extends_does_not_bind_a_parent_inside_an_uncalled_function() {
    let source = concat!(
        "to make()\n",
        "    object Person\n",
        "        has age default 1\n",
        "    end\n",
        "end\n",
        "object Employee extends Person\n",
        "end\n",
        "say Employee.age\n",
    );

    assert_eq!(
        goto_definition(source, 8, 14).expect("a position inside the file is valid"),
        None,
        "the interpreter refuses a parent no call has declared, so navigation must not inherit from it"
    );
    assert!(
        find_references(source, 8, 14)
            .expect("a position inside the file is valid")
            .is_empty(),
        "a parent buried in an uncalled function is not a parent binding, so the member has no references"
    );
}

#[test]
fn edge_extends_does_not_bind_a_parent_inside_an_untaken_branch() {
    let source = concat!(
        "if no then\n",
        "    object Person\n",
        "        has age default 1\n",
        "    end\n",
        "end\n",
        "object Employee extends Person\n",
        "end\n",
        "say Employee.age\n",
    );

    assert_eq!(
        goto_definition(source, 8, 14).expect("a position inside the file is valid"),
        None,
        "the interpreter refuses a parent no run has declared, so navigation must not inherit from it"
    );
    assert!(
        find_references(source, 8, 14)
            .expect("a position inside the file is valid")
            .is_empty(),
        "a parent buried in an untaken branch is not a parent binding, so the member has no references"
    );
}

#[test]
fn edge_this_read_does_not_claim_a_field_a_child_overrides() {
    let source = concat!(
        "object Person\n",
        "    has age default 1\n",
        "    to can describe()\n",
        "        give back this.age\n",
        "    end\n",
        "end\n",
        "object Child extends Person\n",
        "    has age default 2\n",
        "end\n",
        "say Child.describe()\n",
    );

    assert_eq!(
        goto_definition(source, 4, 24).expect("a position inside the file is valid"),
        None,
        "the method runs against Child, whose own age shadows Person's, so the read must not claim Person's field"
    );
    assert!(
        find_references(source, 4, 24)
            .expect("a position inside the file is valid")
            .is_empty(),
        "a read whose binding depends on the runtime receiver claims no single declaration"
    );
}

#[test]
fn edge_function_declaration_shadows_an_earlier_variable() {
    let source = concat!(
        "set f to 1\n",
        "say f\n",
        "to f()\n",
        "    give back 2\n",
        "end\n",
        "say f\n",
        "say f()\n",
    );

    let declaration = goto_definition(source, 3, 4)
        .expect("a position inside the file is valid")
        .expect("the `to` declares a function");
    assert_eq!(
        (
            declaration.line,
            declaration.column,
            declaration.kind.as_str()
        ),
        (3, 4, "function"),
        "a later function must not reuse the earlier variable's binding"
    );

    let after = goto_definition(source, 6, 5)
        .expect("a position inside the file is valid")
        .expect("the function is in scope after its `end`");
    assert_eq!(
        (after.line, after.column, after.kind.as_str()),
        (3, 4, "function"),
        "a bare read after the function declaration names the function"
    );

    let variable_references: Vec<(usize, usize)> = find_references(source, 1, 5)
        .expect("a position inside the file is valid")
        .into_iter()
        .map(|location| (location.line, location.column))
        .collect();
    assert_eq!(
        variable_references,
        vec![(1, 5), (2, 5)],
        "the variable's references must stop at the function declaration"
    );

    let function_references: Vec<(usize, usize)> = find_references(source, 3, 4)
        .expect("a position inside the file is valid")
        .into_iter()
        .map(|location| (location.line, location.column))
        .collect();
    assert_eq!(
        function_references,
        vec![(3, 4), (6, 5), (7, 5)],
        "the function's references must not join the earlier variable"
    );
}

#[test]
fn edge_later_set_shadows_an_earlier_function() {
    let source = concat!(
        "to f()\n",
        "    give back 2\n",
        "end\n",
        "say f\n",
        "set f to 1\n",
        "say f\n",
    );

    let declaration = goto_definition(source, 5, 5)
        .expect("a position inside the file is valid")
        .expect("the `set` declares its own binding");
    assert_eq!(
        (
            declaration.line,
            declaration.column,
            declaration.kind.as_str()
        ),
        (5, 5, "variable"),
        "a later variable must not reuse the earlier function's binding"
    );

    let later = goto_definition(source, 6, 5)
        .expect("a position inside the file is valid")
        .expect("the variable is in scope after the `set`");
    assert_eq!(
        (later.line, later.column, later.kind.as_str()),
        (5, 5, "variable"),
        "a bare read after the `set` names the variable"
    );

    let function_references: Vec<(usize, usize)> = find_references(source, 1, 4)
        .expect("a position inside the file is valid")
        .into_iter()
        .map(|location| (location.line, location.column))
        .collect();
    assert_eq!(
        function_references,
        vec![(1, 4), (4, 5)],
        "the function's earlier uses must not join the later variable"
    );

    let variable_references: Vec<(usize, usize)> = find_references(source, 5, 5)
        .expect("a position inside the file is valid")
        .into_iter()
        .map(|location| (location.line, location.column))
        .collect();
    assert_eq!(
        variable_references,
        vec![(5, 5), (6, 5)],
        "the variable's references start at its own declaration"
    );
}

#[test]
fn edge_call_after_a_set_shadowing_a_function_does_not_resolve() {
    // The runtime raises `Unknown function 'f'` for a call after `set f`, so
    // navigation must not fall back to the function the `set` shadowed.
    let source = concat!(
        "to f()\n",
        "    give back 2\n",
        "end\n",
        "say f\n",
        "set f to 1\n",
        "say f\n",
        "say f()\n",
    );

    assert_eq!(
        goto_definition(source, 7, 5).expect("a position inside the file is valid"),
        None,
        "a call after the shadowing `set` has no function to resolve"
    );

    assert!(
        find_references(source, 7, 5)
            .expect("a position inside the file is valid")
            .is_empty(),
        "the shadowed call is not a use of the earlier function"
    );

    let function_references: Vec<(usize, usize)> = find_references(source, 1, 4)
        .expect("a position inside the file is valid")
        .into_iter()
        .map(|location| (location.line, location.column))
        .collect();
    assert_eq!(
        function_references,
        vec![(1, 4), (4, 5)],
        "the function keeps only its earlier uses"
    );

    let variable_references: Vec<(usize, usize)> = find_references(source, 5, 5)
        .expect("a position inside the file is valid")
        .into_iter()
        .map(|location| (location.line, location.column))
        .collect();
    assert_eq!(
        variable_references,
        vec![(5, 5), (6, 5)],
        "the variable owns the later bare reads but not the call"
    );
}

#[test]
fn edge_set_inside_a_function_shadows_an_outer_function_locally() {
    let source = concat!(
        "to f()\n",
        "    give back 2\n",
        "end\n",
        "to g()\n",
        "    set f to 1\n",
        "    say f\n",
        "end\n",
        "say f\n",
    );

    let local = goto_definition(source, 6, 9)
        .expect("a position inside the file is valid")
        .expect("the local variable is in scope inside `g`");
    assert_eq!(
        (local.line, local.column, local.kind.as_str()),
        (5, 9, "variable"),
        "a `set` inside a function shadows the outer function in that scope"
    );

    let outer = goto_definition(source, 8, 5)
        .expect("a position inside the file is valid")
        .expect("the outer function survives the local variable");
    assert_eq!(
        (outer.line, outer.column, outer.kind.as_str()),
        (1, 4, "function"),
        "the local variable must not leak past its function's `end`"
    );

    assert_eq!(
        find_references(source, 5, 9)
            .expect("a position inside the file is valid")
            .into_iter()
            .map(|location| (location.line, location.column))
            .collect::<Vec<_>>(),
        vec![(5, 9), (6, 9)],
        "the local binding's references stay inside `g`"
    );
}

#[test]
fn edge_function_variable_shadowing_rejects_a_position_past_the_line() {
    let source = concat!("set f to 1\n", "to f()\n", "    give back 2\n", "end\n",);

    assert!(
        goto_definition(source, 1, 99).is_err(),
        "a column past the line is a clean error from goto-definition"
    );
    assert!(
        find_references(source, 1, 99).is_err(),
        "a column past the line is a clean error from find-references"
    );
    assert!(
        completions(source, 1, 99).is_err(),
        "a column past the line is a clean error from completion"
    );
}

#[test]
fn edge_set_inside_a_branch_shadows_a_function_in_the_runtime_frame() {
    let source = concat!(
        "to f()\n",
        "    give back 2\n",
        "end\n",
        "if yes then\n",
        "    set f to 1\n",
        "    say f\n",
        "end\n",
        "say f\n",
    );

    let variable = goto_definition(source, 8, 5)
        .expect("a position inside the file is valid")
        .expect("the branch's `set` replaces the global function for later reads");
    assert_eq!(
        (variable.line, variable.column, variable.kind.as_str()),
        (5, 9, "variable"),
        "a block scope writes a `set` through to the program frame"
    );

    assert_eq!(
        find_references(source, 5, 9)
            .expect("a position inside the file is valid")
            .into_iter()
            .map(|location| (location.line, location.column))
            .collect::<Vec<_>>(),
        vec![(5, 9), (6, 9), (8, 5)],
        "the variable owns the branch read and the read after `end`"
    );

    let nested = concat!(
        "to f()\n",
        "    give back 2\n",
        "end\n",
        "to g()\n",
        "    if yes then\n",
        "        set f to 1\n",
        "    end\n",
        "    say f\n",
        "end\n",
        "say f\n",
    );

    let local = goto_definition(nested, 8, 9)
        .expect("a position inside the file is valid")
        .expect("the branch's `set` lands in `g`'s frame");
    assert_eq!(
        (local.line, local.column, local.kind.as_str()),
        (6, 13, "variable"),
        "a branch inside a function writes to that function's frame"
    );

    let outer = goto_definition(nested, 10, 5)
        .expect("a position inside the file is valid")
        .expect("the function survives because the local variable did not leak");
    assert_eq!(
        (outer.line, outer.column, outer.kind.as_str()),
        (1, 4, "function"),
        "the local variable must not escape `g`"
    );
}

#[test]
fn edge_set_shadows_a_function_declared_in_the_same_branch() {
    // A function declared inside an `if` body lives in that body's scope. A
    // later `set` in the same body replaces the function's runtime slot, so
    // the read after it must answer the variable rather than the stale
    // function entry the block still holds.
    let source = concat!(
        "if yes then\n",
        "    to f()\n",
        "        give back 2\n",
        "    end\n",
        "    say f()\n",
        "    set f to 1\n",
        "    say f\n",
        "end\n",
    );

    let variable = goto_definition(source, 7, 9)
        .expect("a position inside the file is valid")
        .expect("the `set` replaces the branch's function");
    assert_eq!(
        (variable.line, variable.column, variable.kind.as_str()),
        (6, 9, "variable"),
        "a read after the branch's `set` names the replacing variable"
    );

    assert_eq!(
        find_references(source, 6, 9)
            .expect("a position inside the file is valid")
            .into_iter()
            .map(|location| (location.line, location.column))
            .collect::<Vec<_>>(),
        vec![(6, 9), (7, 9)],
        "the variable owns the read after the `set`"
    );

    assert_eq!(
        find_references(source, 2, 8)
            .expect("a position inside the file is valid")
            .into_iter()
            .map(|location| (location.line, location.column))
            .collect::<Vec<_>>(),
        vec![(2, 8), (5, 9)],
        "the function keeps its own declaration and the call before the `set`"
    );
}

#[test]
fn edge_call_after_a_set_does_not_reach_a_later_function() {
    // The runtime holds the `set`'s value in the shared slot, so a call before
    // a later `to f()` raises `Unknown function 'f'`. Navigation must not jump
    // forward past the shadowing variable to the later declaration.
    let source = concat!(
        "to f()\n",
        "    give back 2\n",
        "end\n",
        "set f to 1\n",
        "say f()\n",
        "to f()\n",
        "    give back 3\n",
        "end\n",
    );

    assert_eq!(
        goto_definition(source, 5, 5).expect("a position inside the file is valid"),
        None,
        "a call after the `set` must not reach the later function"
    );
    assert!(
        find_references(source, 5, 5)
            .expect("a position inside the file is valid")
            .is_empty(),
        "the unresolved call is not a use of either function declaration"
    );

    let later = goto_definition(source, 6, 4)
        .expect("a position inside the file is valid")
        .expect("the later `to` declares its own function");
    assert_eq!(
        (later.line, later.column, later.kind.as_str()),
        (6, 4, "function"),
        "the later function declaration still resolves to itself"
    );
}
