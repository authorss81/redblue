use std::fs;
use std::path::{Path, PathBuf};

use redblue::lexer::{Lexer, TokenKind};
use redblue::{run_file, Error, Span};

fn lex(source: &str) -> Vec<TokenKind> {
    Lexer::tokenize(source)
        .expect("source should lex")
        .into_iter()
        .map(|token| token.kind)
        .collect()
}

fn lex_error(source: &str) -> Error {
    Lexer::tokenize(source).expect_err("source should fail to lex")
}

fn count_newlines(kinds: &[TokenKind]) -> usize {
    kinds
        .iter()
        .filter(|kind| **kind == TokenKind::Newline)
        .count()
}

/// Scratch directory inside the project's own `target/tmp`, never the system
/// temp dir, so nothing outside the checkout is touched. Each caller gets its
/// own subdirectory because tests run in parallel.
fn scratch_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/tmp/rb-lexer-robustness")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch dir should be creatable");
    dir
}

// ---------------------------------------------------------------------------
// Unterminated strings
// ---------------------------------------------------------------------------

#[test]
fn test_unterminated_string_is_a_spanned_lexer_error() {
    let source = "say \"hello\n";

    let error = lex_error(source);

    assert!(
        matches!(error, Error::Lexer(..)),
        "an unterminated string is a lexer error, got: {}",
        error
    );
    assert_eq!(
        error.span(),
        Some(&Span::new(1, 5)),
        "the error must point at the opening quote, got: {:?}",
        error.span()
    );
    assert!(
        error.to_string().contains("Unterminated"),
        "the message must say what is wrong, got: {}",
        error
    );
}

#[test]
fn edge_unterminated_string_reports_the_line_the_quote_opened_on() {
    // The quote opens on line 2; the input ends on line 3. The error belongs to
    // line 2, where the reader can see the mistake.
    let source = "set x to 1\nsay \"never closed\nset y to 2\n";

    let error = lex_error(source);

    assert_eq!(
        error.span(),
        Some(&Span::new(2, 5)),
        "the error must name the line the quote opened on, got: {:?}",
        error.span()
    );
}

#[test]
fn edge_escape_at_end_of_input_inside_a_string_is_still_an_error() {
    // The text ends on a lone backslash: the escape has nothing to escape.
    let error = lex_error("say \"abc\\");

    assert!(
        matches!(error, Error::Lexer(..)),
        "a dangling escape is a lexer error, got: {}",
        error
    );
    assert_eq!(error.span(), Some(&Span::new(1, 5)));
}

// ---------------------------------------------------------------------------
// Line endings
// ---------------------------------------------------------------------------

#[test]
fn test_crlf_line_endings_produce_one_newline_per_line() {
    let kinds = lex("set x to 1\r\nsay x\r\n");

    assert_eq!(
        count_newlines(&kinds),
        2,
        "CRLF is one line break, not two, got: {:?}",
        kinds
    );
    assert_eq!(
        kinds,
        vec![
            TokenKind::Set,
            TokenKind::Identifier("x".to_string()),
            TokenKind::To,
            TokenKind::Number(1.0),
            TokenKind::Newline,
            TokenKind::Say,
            TokenKind::Identifier("x".to_string()),
            TokenKind::Newline,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn edge_lone_carriage_return_ends_a_line() {
    // A lone CR is how a classic Mac file separates statements. Dropping it
    // would glue `set x to 1` and `say 1` into one line with no break at all.
    let kinds = lex("set x to 1\rsay 1\r");

    assert_eq!(
        count_newlines(&kinds),
        2,
        "a lone CR must end the line, got: {:?}",
        kinds
    );
}

#[test]
fn edge_cr_does_not_inflate_the_line_counter() {
    let source = "set x to 1\r\nsay @\r\n";

    let error = lex_error(source);

    assert_eq!(
        error.span(),
        Some(&Span::new(2, 5)),
        "the CR of a CRLF must not count as a line, got: {:?}",
        error.span()
    );
}

// ---------------------------------------------------------------------------
// Byte-order mark
// ---------------------------------------------------------------------------

#[test]
fn test_bom_is_stripped_and_not_lexed_as_an_identifier() {
    let kinds = lex("\u{FEFF}say \"hi\"\n");

    assert_eq!(
        kinds,
        vec![
            TokenKind::Say,
            TokenKind::Text("hi".to_string()),
            TokenKind::Newline,
            TokenKind::Eof,
        ],
        "a leading BOM must be invisible to the tokenizer, got: {:?}",
        kinds
    );
}

#[test]
fn edge_bom_before_each_line_is_stripped() {
    // Some editors write the mark on every line. It is not a character the
    // language has, so it must not become part of the following identifier.
    let kinds = lex("\u{FEFF}set x to 1\u{FEFF}\nsay x\u{FEFF}\n");

    assert_eq!(
        kinds,
        vec![
            TokenKind::Set,
            TokenKind::Identifier("x".to_string()),
            TokenKind::To,
            TokenKind::Number(1.0),
            TokenKind::Newline,
            TokenKind::Say,
            TokenKind::Identifier("x".to_string()),
            TokenKind::Newline,
            TokenKind::Eof,
        ],
        "a BOM may not glue itself onto an identifier, got: {:?}",
        kinds
    );
}

// ---------------------------------------------------------------------------
// NUL and control characters
// ---------------------------------------------------------------------------

#[test]
fn test_nul_byte_is_rejected_with_a_column() {
    let source = "set x to 1\0\n";

    let error = lex_error(source);

    assert!(
        matches!(error, Error::Lexer(..)),
        "a NUL byte is not a character, got: {}",
        error
    );
    assert_eq!(
        error.span(),
        Some(&Span::new(1, 11)),
        "the error must point at the NUL, got: {:?}",
        error.span()
    );
}

#[test]
fn edge_control_character_message_is_readable() {
    // The raw NUL is invisible in a terminal, so a message that quotes the byte
    // itself tells the reader nothing.
    let error = lex_error("set x to 1\0\n");

    let rendered = error.to_string();
    assert!(
        rendered.contains("\\0"),
        "the message must escape the byte, got: {:?}",
        rendered
    );
    assert!(
        !rendered.contains('\0'),
        "the message must not contain a raw NUL, got: {:?}",
        rendered
    );
}

#[test]
fn edge_other_control_characters_are_rejected_with_a_column() {
    // BEL and ESC are control characters, not whitespace and not identifiers.
    for (source, column) in [("say \u{7}\n", 5), ("set x to 1\u{1B}[0m\n", 11)] {
        let error = lex_error(source);
        assert_eq!(
            error.span(),
            Some(&Span::new(1, column)),
            "{:?} must be rejected at its column",
            source
        );
    }
}

// ---------------------------------------------------------------------------
// Empty input
// ---------------------------------------------------------------------------

#[test]
fn edge_empty_source_lexes_to_only_eof() {
    assert_eq!(lex(""), vec![TokenKind::Eof]);

    // Whitespace and comments only: no tokens but the line breaks and the end
    // of input.
    let kinds = lex("  \t// just a comment\n\n");
    assert_eq!(
        kinds,
        vec![TokenKind::Newline, TokenKind::Newline, TokenKind::Eof]
    );
}

// ---------------------------------------------------------------------------
// Unicode inside strings
// ---------------------------------------------------------------------------

#[test]
fn test_unicode_round_trips_through_strings() {
    let text = "😀 你好 שלום é \u{200F}";

    let kinds = lex(&format!("say \"{}\"\n", text));

    assert_eq!(
        kinds,
        vec![
            TokenKind::Say,
            TokenKind::Text(text.to_string()),
            TokenKind::Newline,
            TokenKind::Eof,
        ],
        "every code point inside a string must survive unchanged, got: {:?}",
        kinds
    );
}

#[test]
fn edge_column_after_multibyte_characters_counts_characters() {
    // The bad character follows four emoji plus a space: character column 10,
    // even though the byte offset is far larger.
    let source = "say \"😀😀😀😀\" @\n";

    let error = lex_error(source);

    assert_eq!(
        error.span(),
        Some(&Span::new(1, 12)),
        "columns count characters, not bytes, got: {:?}",
        error.span()
    );
}

// ---------------------------------------------------------------------------
// Invalid UTF-8 on disk
// ---------------------------------------------------------------------------

#[test]
fn edge_invalid_utf8_file_is_an_io_error_not_a_panic() {
    let dir = scratch_dir("bad_utf8");
    let path = dir.join("bad_utf8.rb");
    fs::write(&path, b"say \"\xff\xfe\"\n").expect("scratch file should be writable");

    let error = run_file(path.to_str().expect("utf-8 path")).expect_err("must fail");

    assert!(
        matches!(error, Error::Io(..)),
        "undecodable bytes are an io error, got: {}",
        error
    );
    assert_eq!(
        error.span(),
        None,
        "an unreadable file has no source position"
    );
    assert!(error.to_string().starts_with("IoError:"), "got: {}", error);
}

#[test]
fn edge_bom_and_crlf_file_runs_from_disk() {
    let dir = scratch_dir("bom_crlf");
    let path = dir.join("bom_crlf.rb");
    fs::write(&path, b"\xef\xbb\xbfsay \"hi\"\r\nsay \"there\"\r\n").expect("writable");

    assert_eq!(
        run_file(path.to_str().expect("utf-8 path")).map_err(|e| e.to_string()),
        Ok(()),
        "a Windows file with a BOM must run unchanged"
    );
}
