//! Diagnostics: a failure should say what to write, not only where it is.
//!
//! Every error in this interpreter already carries a position, so a reader is
//! told *where* they went wrong and nothing else. These tests pin the two things
//! that make a diagnostic worth reading: the name the writer probably meant, and
//! the offending source lines when the offending thing is more than one line.

use redblue::{run_source, Error};

fn failure_of(source: &str) -> Error {
    run_source(source).expect_err("expected the program to fail")
}

// ---------------------------------------------------------------------------
// did-you-mean
// ---------------------------------------------------------------------------

#[test]
fn test_mistyped_keyword_suggests_the_intended_keyword() {
    // `sya` is a keyword with two letters transposed, which is the single most
    // common way to mistype one. It lexes as an identifier, so without a
    // suggestion the reader is told they used an unknown *variable* — which is
    // true and useless.
    let error = failure_of("sya \"hi\"\n");

    assert_eq!(
        error.label(),
        "AnalyzerError",
        "a mistyped keyword lexes as an identifier, so it is an analyzer failure"
    );
    assert_eq!(
        error.message(),
        "Unknown variable 'sya'; did you mean 'say'?",
        "a mistyped keyword must name the keyword that was meant"
    );
}

#[test]
fn test_near_miss_identifier_suggests_the_declared_name() {
    // One character short of a name the program has already bound.
    let error = failure_of("set count to 3\nsay coun\n");

    assert_eq!(
        error.message(),
        "Unknown variable 'coun'; did you mean 'count'?",
        "a near-miss identifier must name the declared variable that was meant"
    );
}

#[test]
fn edge_transposed_and_missing_letters_both_count_as_one_mistake() {
    // A transposition is one mistake, not two: `adn` for `and` and `tehn` for
    // `then` are both a single slip of the fingers, and a distance that charges
    // twice for them refuses to offer the fix.
    assert_eq!(
        failure_of("set flag to adn\nsay flag\n").message(),
        "Unknown variable 'adn'; did you mean 'and'?",
        "a transposed pair is one edit away"
    );
    assert_eq!(
        failure_of("set flag to tehn\nsay flag\n").message(),
        "Unknown variable 'tehn'; did you mean 'then'?",
        "a transposed keyword is one edit away"
    );
    assert_eq!(
        failure_of("set flag to mport\nsay flag\n").message(),
        "Unknown variable 'mport'; did you mean 'import'?",
        "a dropped letter is one edit away"
    );
}

#[test]
fn edge_a_name_with_no_near_match_is_reported_with_no_suggestion() {
    // A suggestion is only worth making when it is likely right. An invented one
    // sends the reader hunting for a typo that is not there.
    let error = failure_of("say zzzzzzzzz\n");

    assert_eq!(
        error.message(),
        "Unknown variable 'zzzzzzzzz'",
        "a name far from every candidate must be reported unchanged, got: {}",
        error
    );
}

#[test]
fn edge_a_one_character_name_is_never_guessed_at() {
    // Every single-letter name is one edit from every other, so a suggestion
    // here is noise. `Unknown variable 'i'` after a `for` body has ended is the
    // canonical case and its message is pinned elsewhere; this keeps the rule
    // that makes that pinning true.
    let error = failure_of("set q to 1\nsay z\n");

    assert_eq!(
        error.message(),
        "Unknown variable 'z'",
        "a one-character name must be reported unchanged, got: {}",
        error
    );
}

#[test]
fn edge_a_suggestion_does_not_replace_the_name_it_is_about() {
    // The suggestion is appended, so the name the reader actually wrote is still
    // in the message. A diagnostic that reports only the fix makes a search for
    // the mistake itself come up empty.
    let error = failure_of("sya \"hi\"\n");
    let message = error.message();

    assert!(
        message.contains("'sya'"),
        "the message must still name what was written, got: {}",
        message
    );
    assert!(
        message.contains("'say'"),
        "the message must also name what was meant, got: {}",
        message
    );
    assert_eq!(
        message.matches('\'').count(),
        4,
        "exactly two names, each in one pair of quotes, got: {}",
        message
    );
}

#[test]
fn edge_a_suggested_name_is_still_rejected_when_written() {
    // A suggestion is advice about the source. Nothing about the language
    // changes: the program is still refused, and a name one edit from a real one
    // still does not resolve.
    let error = failure_of("set count to 3\nsay coun\n");

    assert!(
        run_source("set count to 3\nsay count\n").is_ok(),
        "the suggested program must actually run"
    );
    assert!(
        run_source("set count to 3\nsay coun\n").is_err(),
        "the suggested name is a fix for the reader to apply, not a rule"
    );
    assert_eq!(
        error.span().map(|s| (s.line, s.column)),
        Some((2, 1)),
        "suggesting a fix must not move the reported position, got: {:?}",
        error.span()
    );
}

#[test]
fn test_a_suggestion_survives_into_the_rendered_diagnostic() {
    // `render` is what a user actually sees, and it prints `message()`.
    let source = "sya \"hi\"\n";
    let rendered = failure_of(source).render(source, Some("typo.rb"));

    assert!(
        rendered.starts_with("AnalyzerError: Unknown variable 'sya'; did you mean 'say'?"),
        "the rendered first line must carry the suggestion, got: {}",
        rendered
    );
}

// ---------------------------------------------------------------------------
// rendered multi-line diagnostics
// ---------------------------------------------------------------------------

#[test]
fn test_multiline_span_renders_every_offending_line() {
    // An unterminated string runs until the end of input, so the offending
    // thing is three lines. Showing only the line it opened on points at the
    // opening quote and says nothing about where the string was supposed to end.
    let source = "set s to \"hello\nworld\nsay s\n";
    let rendered = failure_of(source).render(source, Some("str.rb"));

    let expected = "\
LexerError: Unterminated string
  --> str.rb:1:10
1 | set s to \"hello
2 | world
3 | say s
  |          ^~~~~~~";

    assert_eq!(
        rendered, expected,
        "a span covering three lines must render all three, with the underline on the first"
    );
}
#[test]
fn edge_multiline_span_past_the_end_of_the_file_is_clamped() {
    // The string runs to end of input, which is one line past the last line the
    // file has. The renderer must not read past the end or invent a line.
    let source = "say \"abc\n";
    let rendered = failure_of(source).render(source, Some("one.rb"));

    assert_eq!(
        rendered,
        "\
LexerError: Unterminated string
  --> one.rb:1:5
1 | say \"abc
  |     ^~~~~",
        "a single-line file renders one line, got: {}",
        rendered
    );
}

#[test]
fn edge_a_single_line_span_is_still_rendered_on_one_line() {
    // The one-line form is what every other diagnostic in the project uses, and
    // what editor tooling and the corpus compare against. Adding a range must
    // not change it.
    let source = "set x to 1\nsay x +\n";
    let rendered = failure_of(source).render(source, Some("bad.rb"));

    assert_eq!(
        rendered,
        "\
ParserError: Unexpected token Newline
  --> bad.rb:2:8
2 | say x +
  |        ^",
        "a span on one line keeps the one-line form, got: {}",
        rendered
    );
}

// ---------------------------------------------------------------------------
// the taxonomy this phase may not change
// ---------------------------------------------------------------------------

#[test]
fn edge_the_error_variants_are_unchanged() {
    // This phase adds suggestions and rendering. It may not add, remove or
    // rename a variant: `Error::{Lexer,Parser,Analyzer,Runtime,Io}` is the
    // public taxonomy `redblue::Error` promises, and a caller matching on it
    // would break.
    let labels = [
        (
            Error::Lexer("m".to_string(), redblue::Span::new(1, 1)),
            "LexerError",
        ),
        (
            Error::Parser("m".to_string(), redblue::Span::new(1, 1)),
            "ParserError",
        ),
        (
            Error::Analyzer("m".to_string(), redblue::Span::new(1, 1)),
            "AnalyzerError",
        ),
        (
            Error::Runtime("m".to_string(), redblue::Span::new(1, 1)),
            "RuntimeError",
        ),
        (Error::Io("m".to_string()), "IoError"),
    ];

    for (error, expected) in labels {
        assert_eq!(
            error.label(),
            expected,
            "the label a caller matches on must not drift"
        );
    }
}

// ---------------------------------------------------------------------------
// the renderer, driven directly
// ---------------------------------------------------------------------------

/// A failure with a hand-built span, so every shape of range can be drawn
/// without waiting for a lexer error to happen to produce one.
fn ranged(span: redblue::Span) -> Error {
    Error::Runtime("boom".to_string(), span)
}

#[test]
fn test_a_two_line_range_draws_two_lines() {
    let source = "one\ntwo\nthree\n";
    let rendered = ranged(redblue::Span::new(2, 2).extended_to(3, 3)).render(source, Some("r.rb"));

    assert_eq!(
        rendered,
        "\
RuntimeError: boom
  --> r.rb:2:2
2 | two
3 | three
  |  ^~~",
        "a two-line range draws its two lines, got: {}",
        rendered
    );
}

#[test]
fn test_a_range_wide_enough_to_widen_the_gutter_is_aligned() {
    // Line 10 through line 12: the gutter is two digits wide on every line, so
    // the underline lines up under the source rather than one space left of it.
    let source = (1..=12)
        .map(|n| format!("line {}", n))
        .collect::<Vec<_>>()
        .join("\n");
    let source = format!("{}\n", source);
    let rendered =
        ranged(redblue::Span::new(10, 6).extended_to(12, 1)).render(&source, Some("w.rb"));
    let lines: Vec<&str> = rendered.lines().collect();

    assert_eq!(lines[1], "  --> w.rb:10:6", "the location");
    assert_eq!(lines[2], "10 | line 10", "the first line of the range");
    assert_eq!(lines[3], "11 | line 11", "the middle line of the range");
    assert_eq!(lines[4], "12 | line 12", "the last line of the range");
    assert_eq!(
        lines[5], "   |      ^~~",
        "the underline is padded to the two-digit gutter, got: {:?}",
        lines[5]
    );
}

#[test]
fn edge_a_range_past_the_last_line_is_clamped_to_the_lines_there_are() {
    let source = "one\ntwo\n";
    let rendered = ranged(redblue::Span::new(2, 1).extended_to(99, 1)).render(source, Some("c.rb"));

    assert_eq!(
        rendered,
        "\
RuntimeError: boom
  --> c.rb:2:1
2 | two
  | ^~~~",
        "a range past the end must print only the lines that exist, got: {}",
        rendered
    );
}

#[test]
fn edge_an_end_on_the_start_line_is_not_a_range() {
    // A span that opens and closes on one line has nothing extra to show, so it
    // is drawn the way every other one-line diagnostic has always been drawn.
    let span = redblue::Span::new(2, 2).extended_to(2, 4);
    assert!(
        !span.is_multiline(),
        "an end on the start line covers one line, not a range"
    );

    let source = "one\ntwo words\n";
    let rendered = ranged(span).render(source, Some("s.rb"));

    assert_eq!(
        rendered,
        "\
RuntimeError: boom
  --> s.rb:2:2
2 | two words
  |  ^",
        "a one-line span keeps the caret and gains no underline, got: {}",
        rendered
    );
}

#[test]
fn edge_a_range_opening_past_the_end_of_its_line_underlines_one_character() {
    // The column is one past the last character, which is what an empty
    // offender looks like: the caret still has to be somewhere, and the
    // underline must not run backwards under the source.
    let source = "one\nab\n";
    let rendered = ranged(redblue::Span::new(2, 3).extended_to(4, 1)).render(source, Some("p.rb"));

    assert_eq!(
        rendered,
        "\
RuntimeError: boom
  --> p.rb:2:3
2 | ab
  |   ^",
        "a caret past the last character underlines only itself, got: {}",
        rendered
    );
}

#[test]
fn edge_a_multibyte_line_is_underlined_by_character_and_not_by_byte() {
    // The emoji is one character and four bytes, so a range opening at the
    // character after it underlines by character too.
    let source = "say \"\u{1F600}\u{1F600}oops\nmore\n";
    let error = failure_of(source);
    let span = error.span().expect("the failure carries a span");

    assert_eq!(
        (span.line, span.column, span.is_multiline()),
        (1, 5, true),
        "the span is measured in characters: the quote is the fifth, not the\n         thirty-fourth byte, got: {:?}",
        span
    );
    assert_eq!(
        error.render(source, Some("emoji.rb")),
        format!(
            "LexerError: Unterminated string\n  --> emoji.rb:1:5\n1 | say \"\u{1F600}\u{1F600}oops\n2 | more\n  |     ^~~~~~~~"
        ),
        "the underline sits under the fifth character, not the thirty-fourth byte"
    );
}

#[test]
fn edge_a_crlf_file_breaks_the_same_way_as_a_lf_file() {
    // A file written on Windows carries `\r` at the end of every line. The
    // range has to break on the same lines a reader sees in an editor, not on
    // the byte after the `\r`.
    let source = "set s to \"oops\r\nmore\r\nsay s\r\n";
    let rendered = failure_of(source).render(source, Some("win.rb"));

    assert_eq!(
        rendered,
        "\
LexerError: Unterminated string
  --> win.rb:1:10
1 | set s to \"oops
2 | more
3 | say s
  |          ^~~~~~",
        "a CRLF file renders the same three lines, got: {}",
        rendered
    );
}

// ---------------------------------------------------------------------------
// candidates
// ---------------------------------------------------------------------------

#[test]
fn test_a_name_from_an_outer_scope_is_suggested_inside_a_nested_body() {
    // The candidate list is every scope the walk can see, not only the innermost
    // one: a name bound three frames out is still a name the writer meant.
    let error = failure_of(
        "set total to 1\nfor each item in [1]\n    if item is 1 then\n        say tota\n    end\nend\n",
    );

    assert_eq!(
        error.message(),
        "Unknown variable 'tota'; did you mean 'total'?",
        "a name from the outermost scope must still be offered, got: {}",
        error
    );
}

#[test]
fn edge_a_name_declared_after_the_read_is_not_suggested_as_if_it_were_in_scope() {
    // `later` is the analyzer's separate answer to "the program claims this name
    // somewhere"; it is not a scope. A suggestion drawn from it would offer a
    // name the reader cannot see from where they are.
    let error = failure_of("say coun\nset count to 1\n");

    assert_eq!(
        error.message(),
        "Unknown variable 'coun'",
        "a name bound below the read is not in scope at the read, got: {}",
        error
    );
}

#[test]
fn edge_a_unicode_name_typo_is_suggested_in_its_own_script() {
    // Identifiers are alphabetic characters, not ASCII letters, so a name
    // written in another script is a name. The distance is measured in
    // characters, and the suggestion is the name as it was spelled.
    let error = failure_of("set caf\u{e9} to 1\nsay caf\u{e8}\n");

    assert_eq!(
        error.message(),
        "Unknown variable 'caf\u{e8}'; did you mean 'caf\u{e9}'?",
        "one accent away is one character away, got: {}",
        error
    );
}

#[test]
fn edge_candidates_are_offered_the_same_way_however_often_the_program_is_run() {
    // The candidates come out of a `HashSet`, whose iteration order is not
    // promised. Two runs of the same program must name the same thing: a
    // diagnostic that changes between runs cannot be copied into a bug report.
    let source = "set alpha to 1\nset beta to 2\nsay alpga\n";
    let first = failure_of(source).message().to_string();

    for _ in 0..16 {
        assert_eq!(
            failure_of(source).message(),
            first,
            "the suggestion must not depend on hash iteration order"
        );
    }
}

#[test]
fn edge_a_duplicate_name_in_two_scopes_suggests_the_same_thing() {
    // The candidate list is deduplicated. A name shadowed across three frames
    // is one name, and offering it three times would not change the answer — but
    // a list that grew with depth would make the answer depend on how deep the
    // reader is.
    let error = failure_of(
        "set count to 1\nfor each count in [1]\n    if count is 1 then\n        say coun\n    end\nend\n",
    );

    assert_eq!(
        error.message(),
        "Unknown variable 'coun'; did you mean 'count'?",
        "a name bound at several depths is one candidate, got: {}",
        error
    );
}
