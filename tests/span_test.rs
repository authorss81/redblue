use redblue::{run_source, Error, Span};

fn error_of(source: &str) -> Error {
    run_source(source).expect_err("expected the program to fail")
}

// ---------------------------------------------------------------------------
// Parser errors
// ---------------------------------------------------------------------------

#[test]
fn test_parser_error_reports_line_and_column() {
    let source = "set x to 1\nsay x +\n";
    let error = error_of(source);
    let rendered = error.to_string();

    assert!(
        rendered.contains("2:8"),
        "parser error must report line:column 2:8, got: {}",
        rendered
    );
    assert!(
        matches!(error.span(), Some(span) if *span == Span::new(2, 8)),
        "parser error must carry the offending token position, got: {:?}",
        error.span()
    );
}

#[test]
fn edge_error_on_first_line_points_at_line_one() {
    let source = "say x +\nset y to 2\n";
    let error = error_of(source);

    assert_eq!(
        error.span(),
        Some(&Span::new(1, 8)),
        "an error on the first line must report line 1"
    );
}

#[test]
fn edge_error_on_last_line_points_at_the_last_line() {
    let source = "set x to 1\nset y to 2\nsay y *\n";
    let error = error_of(source);

    assert_eq!(
        error.span().map(|span| span.line),
        Some(3),
        "an error on the last line must report that line"
    );
}

#[test]
fn edge_empty_source_is_a_valid_empty_program() {
    assert_eq!(
        run_source("").map_err(|e| e.to_string()),
        Ok(()),
        "an empty file is an empty program, not a failure"
    );

    // A file that is only a stray token is the smallest malformed input, and
    // it has no source line to echo.
    let error = error_of("end");
    assert_eq!(
        error.span(),
        Some(&Span::new(1, 1)),
        "a stray token still has a position, got: {:?}",
        error.span()
    );
    assert!(
        error.to_string().contains("Unexpected token"),
        "a stray token must be named, got: {}",
        error
    );
}

#[test]
fn edge_multibyte_column_offsets_are_character_based() {
    // The emoji is four bytes but one character, so the offending token sits at
    // character column 8 even though its byte offset is far larger.
    let source = "say \"\u{1F600}\" +\n";
    let error = error_of(source);
    let span = error
        .span()
        .expect("a multi-byte line still has a position");

    // The byte offset of the newline is 13; the character column is 10.
    assert_eq!(
        source.find('\n').unwrap(),
        12,
        "the fixture must contain a multi-byte character"
    );
    assert_eq!(
        *span,
        Span::new(1, 10),
        "columns count characters, not bytes"
    );

    let rendered = error.render(source, Some("emoji.rb"));
    let caret_line = rendered.lines().last().expect("render has a caret line");
    assert_eq!(
        caret_line,
        format!("  | {}^", " ".repeat(9)),
        "the caret must sit under character column 10, got: {:?}",
        caret_line
    );
    assert!(
        rendered.contains("1 | say \"\u{1F600}\" +"),
        "the offending source line must be echoed, got: {}",
        rendered
    );
}

#[test]
fn test_lexer_error_carries_position() {
    let source = "set x to 1\nset y to @\n";
    let error = error_of(source);

    assert!(
        error.to_string().contains("LexerError"),
        "an unknown character is a lexer error, got: {}",
        error
    );
    assert_eq!(
        error.span(),
        Some(&Span::new(2, 10)),
        "the lexer must report where the character is"
    );
}

// ---------------------------------------------------------------------------
// Analyzer and Runtime errors
// ---------------------------------------------------------------------------

#[test]
fn edge_runtime_error_reports_the_statement_position() {
    let source = "set x to 1\n\nset y to 4 / 0\n";
    let error = error_of(source);

    assert!(
        error.to_string().contains("RuntimeError"),
        "dividing by zero is a runtime error, got: {}",
        error
    );
    assert_eq!(
        error.span(),
        Some(&Span::new(3, 1)),
        "a runtime error must point at its statement, got: {:?}",
        error.span()
    );
}

#[test]
fn edge_runtime_error_inside_a_loop_reports_the_inner_statement() {
    let source = "for each i in [1, 2, 3]\n    say i / 0\nend\n";
    let error = error_of(source);

    assert!(
        error.to_string().contains("Division by zero"),
        "dividing by zero must still be reported, got: {}",
        error
    );
    assert_eq!(
        error.span(),
        Some(&Span::new(2, 5)),
        "the loop body statement owns the position, got: {:?}",
        error.span()
    );
}

#[test]
fn test_analyzer_error_carries_position() {
    let source = "set x to 1\nsay missing\n";
    let error = error_of(source);

    assert!(
        error.to_string().contains("AnalyzerError"),
        "an undefined name is an analyzer error, got: {}",
        error
    );
    assert!(
        error.to_string().contains("missing"),
        "the failing name must be named, got: {}",
        error
    );
    assert_eq!(
        error.span(),
        Some(&Span::new(2, 1)),
        "the analyzer must report where the bad name appears, got: {:?}",
        error.span()
    );
}

// ---------------------------------------------------------------------------
// Diagnostic rendering
// ---------------------------------------------------------------------------

#[test]
fn test_render_draws_source_line_and_caret() {
    let source = "set x to 1\nsay x +\n";
    let rendered = error_of(source).render(source, Some("bad.rb"));
    let lines: Vec<&str> = rendered.lines().collect();

    assert_eq!(
        lines[0], "ParserError: Unexpected token Newline",
        "first line is the message"
    );
    assert_eq!(lines[1], "  --> bad.rb:2:8", "second line is the location");
    assert_eq!(lines[2], "2 | say x +", "third line echoes the source");
    assert_eq!(
        lines[3],
        format!("  | {}^", " ".repeat(7)),
        "fourth line puts the caret under column 8"
    );
}

#[test]
fn edge_render_without_a_file_omits_the_file_name() {
    let source = "set x to 1\nsay x +\n";
    let rendered = error_of(source).render(source, None);

    assert!(
        rendered.contains("--> 2:8"),
        "a source with no file name still reports line:column, got: {}",
        rendered
    );
}

#[test]
fn edge_render_of_a_span_past_the_last_line_still_reports_the_location() {
    // An unclosed `if` is reported at the end of input, which is one line past
    // the last line of the file.
    let source = "set x to 1\nif x then\nsay 2\n";
    let error = error_of(source);
    assert_eq!(error.span(), Some(&Span::new(4, 1)));

    let rendered = error.render(source, Some("short.rb"));

    assert!(
        rendered.contains("--> short.rb:4:1"),
        "the location must survive a missing source line, got: {}",
        rendered
    );
    assert!(
        !rendered.contains('^'),
        "no caret can be drawn without a source line, got: {}",
        rendered
    );
}

#[test]
fn test_io_error_has_no_span() {
    let error =
        redblue::run_file("this-file-does-not-exist.rb").expect_err("a missing file must fail");

    assert_eq!(error.span(), None, "an unreadable file has no position");
    assert!(
        error.to_string().starts_with("IoError:"),
        "a missing file is an io error, got: {}",
        error
    );
}

#[test]
fn edge_unknown_span_renders_message_only() {
    let error = Error::Runtime("something went wrong".to_string(), Span::unknown());
    let rendered = error.render("say 1\n", Some("x.rb"));

    assert_eq!(
        rendered, "RuntimeError: something went wrong",
        "an unknown span has no location to draw, got: {}",
        rendered
    );
}

#[test]
fn test_every_failed_program_reports_a_position() {
    let failures = [
        "say x +\n",
        "set x to 1\nsay x +\n",
        "set\n",
        "for each i in [1]\nsay nope\nend\n",
        "set x to 1\nif x then\nsay 2\n",
        "to add(a, b)\nset c to a + b\ngive back c\nend\nset d to add(1, q)\n",
        "say {1: 2, 3}\n",
    ];

    for source in failures {
        let error = error_of(source);
        assert!(
            error.span().is_some(),
            "`{}` must report a source position, got: {}",
            source.trim(),
            error
        );
    }
}
