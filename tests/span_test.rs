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
fn edge_crlf_source_renders_a_clean_caret_line() {
    let source = "set x to 1\r\nsay x +\r\n";
    let error = error_of(source);

    assert_eq!(
        error.span(),
        Some(&Span::new(2, 9)),
        "a CRLF file must report the same position as an LF file, got: {:?}",
        error.span()
    );

    let rendered = error.render(source, Some("crlf.rb"));
    let lines: Vec<&str> = rendered.lines().collect();

    assert_eq!(
        lines[2], "2 | say x +",
        "the echoed line must not keep its \\r"
    );
    assert_eq!(
        lines[3],
        format!("  | {}^", " ".repeat(8)),
        "the caret line must not keep a stray \\r, got: {:?}",
        lines[3]
    );
    assert!(
        !rendered.contains('\r'),
        "no carriage return may survive into the diagnostic, got: {:?}",
        rendered
    );
}

#[test]
fn edge_nested_block_error_points_at_the_innermost_statement() {
    let source = "for each i in [1, 2]\n    if i is 1 then\n        say i / 0\n    end\nend\n";
    let error = error_of(source);

    assert!(
        error.to_string().contains("Division by zero"),
        "the failure must still be reported, got: {}",
        error
    );
    assert_eq!(
        error.span(),
        Some(&Span::new(3, 9)),
        "the innermost statement owns the position, not the loop, got: {:?}",
        error.span()
    );
}

#[test]
fn edge_nested_analyzer_error_points_at_the_offending_name() {
    let source =
        "for each i in [1, 2]\n    if i is 1 then\n        say undefined_name\n    end\nend\n";
    let error = error_of(source);

    assert!(
        error.to_string().contains("undefined_name"),
        "the undefined name must be named, got: {}",
        error
    );
    assert_eq!(
        error.span(),
        Some(&Span::new(3, 9)),
        "the analyzer must point at the nested statement, got: {:?}",
        error.span()
    );
}

#[test]
fn edge_four_digit_line_number_widens_the_caret_gutter() {
    let mut source = String::new();
    for _ in 1..=999 {
        source.push_str("say 1\n");
    }
    source.push_str("say 1 +\n");

    let error = error_of(&source);
    assert_eq!(
        error.span(),
        Some(&Span::new(1000, 8)),
        "the last line of a long file must be located exactly, got: {:?}",
        error.span()
    );

    let rendered = error.render(&source, Some("big.rb"));
    let lines: Vec<&str> = rendered.lines().collect();

    assert_eq!(lines[1], "  --> big.rb:1000:8", "the location is reported");
    assert_eq!(lines[2], "1000 | say 1 +", "the source line is echoed");
    assert_eq!(
        lines[3],
        format!("{} | {}^", " ".repeat(4), " ".repeat(7)),
        "the caret gutter must grow with the line number, got: {:?}",
        lines[3]
    );
}

#[test]
fn edge_column_past_the_end_of_the_line_does_not_panic() {
    // A span can point past the end of its line (an unterminated construct is
    // reported at the end of input). Rendering must still succeed.
    let error = Error::Parser("Unexpected end of input".to_string(), Span::new(1, 40));
    let rendered = error.render("say 1\n", Some("short.rb"));
    let lines: Vec<&str> = rendered.lines().collect();

    assert_eq!(lines[1], "  --> short.rb:1:40", "the position is reported");
    assert_eq!(lines[2], "1 | say 1", "the short line is echoed");
    assert_eq!(
        lines[3],
        format!("  | {}^", " ".repeat(39)),
        "the caret sits at the reported column, got: {:?}",
        lines[3]
    );
}

#[test]
fn test_mixed_width_unicode_columns_are_counted_in_characters() {
    // Four bytes of emoji, three bytes of CJK and a combining mark before the
    // failure. The column must be a character count, not a byte count.
    let source = "say \"\u{1F600}\u{4F60}\u{597D}\u{0065}\u{0301}\" +\n";
    let error = error_of(source);

    // `say "` is five characters, then the emoji, two CJK characters, a
    // letter and a combining mark are five more, so the newline that ends the
    // broken expression is the fourteenth character — not the twenty-third
    // byte.
    assert_eq!(
        error.span(),
        Some(&Span::new(1, 14)),
        "mixed-width characters must not inflate the column, got: {:?}",
        error.span()
    );

    let rendered = error.render(source, None);
    let caret_line = rendered.lines().last().expect("render has a caret line");
    assert_eq!(
        caret_line,
        format!("  | {}^", " ".repeat(13)),
        "the caret must sit under character column 14, got: {:?}",
        caret_line
    );
}

#[test]
fn test_every_failed_program_reports_a_position() {
    // One failing program per error path. Every one of them must carry a known
    // position, and that position must be inside the file (an unterminated
    // construct is reported at the end of input, which is one line past the
    // last line and nothing further).
    let failures = [
        "set x to 1\nset y to @\n",
        "set x to 1\nend\n",
        "set x to 1\nif x then\nsay 2\n",
        "for each i in [1, 2]\nsay i\n",
        "to add(a, b)\nset c to a + b\n",
        "set x to (1 + \n",
        "set\n",
        "say {1: 2}\n",
        "test\n",
        "say x +\n",
        "set x to 1\nsay x +\n",
        "set x to 1\nsay x + \"a\"\n",
        "say 1 / 0\n",
        "say missing\n",
        "if 1 is 1 then\n    say nope\nend\n",
        "to f()\n    give back q\nend\nsay f()\n",
        "set x to 1\nsay x(2)\n",
        "import NoSuchModule\nsay 1\n",
    ];

    for source in failures {
        let error = error_of(source);
        let label = source.trim().replace('\n', " ; ");

        let span = error
            .span()
            .unwrap_or_else(|| panic!("`{}` must report a source position, got: {}", label, error));

        let last_line = source.lines().count();
        assert!(
            span.line >= 1 && span.line <= last_line + 1,
            "`{}` reported line {} but the file has {} lines, got: {}",
            label,
            span.line,
            last_line,
            error
        );
        assert!(
            span.column >= 1,
            "`{}` reported column {} but columns are 1-based, got: {}",
            label,
            span.column,
            error
        );
    }
}
