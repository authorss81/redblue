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
        format!("  | {}^", " ".repeat(10)),
        "the caret is placed in terminal cells, so it lines up under the `+` \
         rather than under the byte offset — the emoji before it is two cells \
         wide, so column 10 is the eleventh cell, not the tenth",
    );
    assert!(
        rendered.contains("1 | say \"\u{1F600}\" +"),
        "the offending source line must be echoed, got: {}",
        rendered
    );
}

#[test]
fn edge_a_tab_before_the_column_is_a_cell_and_not_a_gap() {
    // A tab is a control character, and a renderer that measures cells by
    // asking whether a character is a control answers 0 for it — which steps
    // the caret over every tab on the line and lands it left of the thing it
    // points at. Both shapes a tab can appear in are here: the caret of a
    // single-line diagnostic and the underline of a range.
    let single = "set x to 1\n\tsay nosuchvar\n";
    let error = error_of(single);
    assert_eq!(
        error.span(),
        Some(&Span::new(2, 6)),
        "the name is the sixth character of the line, tab included"
    );
    let rendered = error.render(single, Some("tabbed.rb"));
    assert_eq!(
        rendered.lines().last(),
        Some(format!("  | {}^", " ".repeat(5)).as_str()),
        "the tab before `nosuchvar` is drawn in one cell, so the caret sits \
         under the `n`:\n{}",
        rendered
    );

    let ranged = "set x to 1\n\t\tsay \"unterminated\n";
    let rendered = error_of(ranged).render(ranged, Some("tabbed.rb"));
    assert_eq!(
        rendered.lines().last(),
        Some(format!("  | {}^~~~~~~~~~~~~~", " ".repeat(6)).as_str()),
        "and the underline of a range opens one cell past each of the two \
         tabs, so it starts under the quote rather than under the first tab:\n{}",
        rendered
    );
}

#[test]
fn edge_a_line_longer_than_the_drawing_limit_is_cut_and_the_caret_lands_on_the_cut() {
    // The renderer draws a bounded prefix of a line: `rb run` hands it whatever
    // `read_to_string` returned, so a file with a megabyte-long line must not be
    // drawn a megabyte at a time. The `@` fails at column 1108, past the cut, so
    // the caret is clamped to the end of the text that *is* drawn rather than
    // indenting itself into a megabyte of spaces.
    let source = format!("say \"{}\" @\n", "a".repeat(1100));
    let error = error_of(&source);
    assert_eq!(
        error.span(),
        Some(&Span::new(1, 1108)),
        "the fixture must fail past the drawing limit, not before it"
    );

    let rendered = error.render(&source, Some("long.rb"));
    let drawn = rendered
        .lines()
        .find(|line| line.starts_with("1 | "))
        .expect("the source line is drawn")
        .trim_start_matches("1 | ");

    assert_eq!(
        drawn.chars().count(),
        1025,
        "1024 cells of the line and the marker that says the rest was left out"
    );
    assert!(drawn.ends_with('\u{2026}'), "the cut is marked: {}", drawn);
    assert_eq!(
        drawn.chars().filter(|c| *c == 'a').count(),
        1020,
        "and every cell of the limit is used"
    );
    assert_eq!(
        rendered.lines().last(),
        Some(format!("  | {}^", " ".repeat(1024)).as_str()),
        "the caret lands where the drawing stops rather than under a column \
         that was never drawn:\n{}",
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
        Some(&Span::new(2, 5)),
        "the analyzer must report where the bad name appears — under `missing`, \
         not under the `say` that read it — got: {:?}",
        error.span()
    );
}

/// The same question asked of a name read from inside a string literal, where
/// the braces have to be resolved before the line can be.
#[test]
fn edge_an_unknown_variable_is_reported_under_the_name_it_names() {
    // Two shapes of the same thing: the name at the head of the line, and the
    // same name read in the middle of an expression. A statement-span caret
    // passes the first and fails the second, which is why both are here.
    for (source, column) in [
        ("say nosuchvar\n", 5),
        ("say 1 + nosuchvar\n", 9),
        ("say \"n is {nosuchvar}\"\n", 12),
        ("if nosuchvar is 1 then\n    say 1\nend\n", 4),
    ] {
        let error = error_of(source);
        assert_eq!(
            error.span(),
            Some(&Span::new(1, column)),
            "the caret belongs under the name in:\n{}",
            source
        );
        let rendered = error.render(source, Some("name.rb"));
        assert_eq!(
            rendered.lines().last(),
            Some(format!("  | {}^", " ".repeat(column - 1)).as_str()),
            "and the drawn caret lands under it:\n{}",
            rendered
        );
    }
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
