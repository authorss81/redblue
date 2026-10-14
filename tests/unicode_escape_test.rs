//! `\uXXXX` in a text literal decodes to the character it names (phase-085).
//!
//! `docs/GRAMMAR.md:80` has always promised
//! `escape = '\' ( 'n' | 't' | 'r' | '"' | '\' | 'u' hex_digit×4 )`, but
//! `src/lexer.rs` matched only the first five and sent everything else through
//! a catch-all that pushed the one character after the backslash. So
//! `say "\u00e9"` printed `u00e9` and `say "A\u0042C"` printed `Au0042C` — a
//! program carrying a unicode escape ran to completion and printed text that
//! was not the text that was written. Silent data corruption, not an error.
//!
//! Every assertion about what a program *prints* runs on **both** engines,
//! because "it works" means the tree-walking VM and the bytecode VM decode
//! identically or not at all. `both_engines_print` runs one and then the other.

use redblue::{analyzer, compile_source, lexer::Lexer, parser};

/// Runs `source` through the tree-walking pipeline, the way `rb run` does.
fn tree_walk(source: &str) -> Result<(Vec<String>, redblue::Value), redblue::Error> {
    let tokens = Lexer::tokenize(source)?;
    let ast = parser::parse(tokens)?;
    analyzer::analyze(&ast)?;
    let (mut vm, result) = redblue::run_isolated(&ast);
    let output = vm.take_output();
    result.map(|value| (output, value))
}

/// Runs `source` through the bytecode compiler and then the bytecode VM, the
/// way `rb compile` then `rb vm` does.
fn bytecode(source: &str) -> Result<(Vec<String>, redblue::Value), redblue::Error> {
    let chunk = compile_source(source)?;
    let mut vm = redblue::bytecode::vm::BytecodeVm::new();
    vm.set_echo(false);
    let result = vm.run(&chunk);
    let output = vm.take_output();
    result.map(|value| (output, value))
}

/// Asserts that both engines printed exactly `expected`.
fn both_engines_print(source: &str, expected: &[&str]) {
    let want: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
    let printed = |result: Result<(Vec<String>, redblue::Value), redblue::Error>, engine: &str| {
        let (lines, _) =
            result.unwrap_or_else(|e| panic!("{source:?} must run on {engine}: {e:?}"));
        lines
    };
    assert_eq!(
        printed(tree_walk(source), "tree-walking VM"),
        want,
        "tree-walking VM"
    );
    assert_eq!(
        printed(bytecode(source), "bytecode VM"),
        want,
        "bytecode VM"
    );
}

/// The one line a program printed, on both engines, asserted as a Rust `&str`.
///
/// The two engines are compared to each other *and* to the Rust literal, so a
/// test can never pass by both of them being wrong in the same way — which is
/// exactly how the silent `u00e9` survived: the bug was in both engines at
/// once, because both read the same token.
fn printed_text(source: &str) -> String {
    let line = |result: Result<(Vec<String>, redblue::Value), redblue::Error>, engine: &str| {
        let (lines, _) =
            result.unwrap_or_else(|e| panic!("{source:?} must run on {engine}: {e:?}"));
        assert_eq!(
            lines.len(),
            1,
            "{source:?} printed one line on {engine}, not {lines:?}"
        );
        lines.into_iter().next().unwrap()
    };
    let tree = line(tree_walk(source), "tree-walking VM");
    let byte = line(bytecode(source), "bytecode VM");
    assert_eq!(tree, byte, "the two engines decoded {source:?} differently");
    tree
}

/// The lexer's error for `source`, or a panic naming what it produced instead.
fn lex_error(source: &str) -> redblue::Error {
    Lexer::tokenize(source).expect_err(&format!("{source:?} must not lex"))
}

/// Asserts the lexer refused `source` with a message naming line and column.
///
/// The rendering is what the writer sees, so it is what carries the position:
/// a refusal that says "invalid escape" without saying *where* leaves a reader
/// guessing at a file with a thousand strings in it.
fn assert_refused_at(source: &str, needle: &str) {
    let error = lex_error(source);
    assert!(
        matches!(error, redblue::Error::Lexer(..)),
        "{source:?} must be a LexerError, not {error:?}"
    );
    let rendered = format!("{error}");
    assert!(
        rendered.contains(needle),
        "{source:?} must report {needle:?}, reported {rendered:?}"
    );
    assert!(
        rendered.contains("\n  --> 1:"),
        "{source:?} must name a line and column, reported {rendered:?}"
    );
}

// --- the decode --------------------------------------------------------------

#[test]
fn unicode_escape_decodes_to_the_character_it_names() {
    assert_eq!(printed_text("say \"\\u0041\"\n"), "A");
}

#[test]
fn unicode_escape_decodes_adjacent_to_ordinary_text() {
    assert_eq!(printed_text("say \"A\\u0042C\"\n"), "ABC");
}

#[test]
fn unicode_escape_survives_interpolation() {
    let source = "set name to \"lice\"\nsay \"\\u0041{name}\"\n";
    both_engines_print(source, &["Alice"]);
}

#[test]
fn existing_escapes_keep_their_behaviour() {
    assert_eq!(printed_text("say \"a\\nb\"\n"), "a\nb");
    assert_eq!(printed_text("say \"a\\tb\"\n"), "a\tb");
    assert_eq!(printed_text("say \"a\\rb\"\n"), "a\rb");
    assert_eq!(printed_text("say \"a\\\"b\"\n"), "a\"b");
    assert_eq!(printed_text("say \"a\\\\b\"\n"), "a\\b");
}

// --- edge cases --------------------------------------------------------------

#[test]
fn edge_an_empty_literal_is_still_empty() {
    assert_eq!(printed_text("say \"\"\n"), "");
}

#[test]
fn edge_the_highest_four_digit_escape_is_the_last_basic_plane_character() {
    // U+FFFF is the largest code point four hex digits can name, and the last
    // character of the Basic Multilingual Plane.
    let printed = printed_text("say \"\\uffff\"\n");
    let mut chars = printed.chars();
    let first = chars.next().expect("the escape printed one character");
    assert_eq!(
        first as u32, 0xFFFF,
        "\\uffff must be U+FFFF, not {printed:?}"
    );
    assert!(
        chars.next().is_none(),
        "\\uffff must be one character, printed {printed:?}"
    );
}

#[test]
fn edge_five_hex_digits_are_an_escape_then_ordinary_text() {
    // `\u` takes exactly four hex digits (docs/GRAMMAR.md:80), so U+1F600 is
    // not one escape — it is `\u1f60` followed by a literal `0`. Written
    // plainly, GRINNING FACE is just the character; the escape is for the
    // code points a file cannot otherwise carry.
    let printed = printed_text("say \"\\u1f600\"\n");
    assert_eq!(
        printed, "\u{1F60}0",
        "\\u1f600 is U+1F60 then a literal zero"
    );
    assert_refused_at("say \"\\u{1f600}\"\n", "Unicode escape");
}

#[test]
fn edge_two_byte_escape_is_one_code_point_not_two_bytes() {
    let printed = printed_text("say \"\\u00e9\"\n");
    let mut chars = printed.chars();
    let first = chars.next().expect("the escape printed one character");
    assert_eq!(
        first as u32, 0xE9,
        "\\u00e9 must be U+00E9, not {printed:?}"
    );
    assert!(
        chars.next().is_none(),
        "\\u00e9 must be one character, printed {printed:?}"
    );
    assert_eq!(
        printed.len(),
        2,
        "U+00E9 is two UTF-8 bytes, printed {printed:?}"
    );
}

#[test]
fn edge_upper_and_lowercase_hex_digits_decode_identically() {
    assert_eq!(
        printed_text("say \"\\u00E9|\"\n"),
        printed_text("say \"\\u00e9|\"\n")
    );
    assert_eq!(
        printed_text("say \"\\uABCD|\"\n"),
        printed_text("say \"\\uabcd|\"\n")
    );
}

#[test]
fn edge_escape_immediately_before_the_closing_quote_is_decoded() {
    assert_eq!(printed_text("say \"x\\u0041\"\n"), "xA");
    assert_eq!(printed_text("say \"\\u0041\"\n"), "A");
}

#[test]
fn edge_a_run_of_escapes_decodes_left_to_right() {
    assert_eq!(printed_text("say \"\\u0041\\u0042\\u0043\"\n"), "ABC");
}

#[test]
fn edge_a_literal_may_mix_escapes_and_raw_unicode() {
    // The escape must not consume or mangle the characters written literally.
    assert_eq!(printed_text("say \"\\u0041é中\"\n"), "Aé中");
}

#[test]
fn edge_an_escaped_quote_after_an_escape_does_not_end_the_literal() {
    assert_eq!(printed_text("say \"\\u0041\\\"b\"\n"), "A\"b");
}

// --- refusals: nothing is ever silently rewritten -----------------------------

#[test]
fn edge_a_bare_backslash_u_is_refused() {
    assert_refused_at("say \"\\u\"\n", "Unicode escape");
}

#[test]
fn edge_non_hex_digits_after_backslash_u_are_refused() {
    assert_refused_at("say \"\\uZZZZ\"\n", "Unicode escape");
}

#[test]
fn edge_a_short_escape_is_refused_not_padded() {
    // `\u00e` is three digits. It must not decode as U+000E or as U+00E0.
    assert_refused_at("say \"\\u00e\"\n", "Unicode escape");
    assert_refused_at("say \"\\u00\"\n", "Unicode escape");
    assert_refused_at("say \"\\u0\"\n", "Unicode escape");
}

#[test]
fn edge_an_escape_at_the_end_of_input_is_refused() {
    assert_refused_at("say \"\\u00e9", "Unterminated string");
}

#[test]
fn edge_a_bare_trailing_backslash_is_refused() {
    assert_refused_at("say \"abc\\", "Unterminated string");
}

#[test]
fn edge_an_unterminated_string_is_refused() {
    assert_refused_at("say \"\\u0041", "Unterminated string");
}

#[test]
fn edge_a_surrogate_half_is_refused() {
    // U+D83D is half of a surrogate pair and names no character of its own. A
    // lone surrogate has no UTF-8 encoding, so decoding it "successfully"
    // would be inventing a character.
    assert_refused_at("say \"\\ud83d\"\n", "Unicode escape");
}

#[test]
fn edge_an_unknown_escape_is_refused() {
    // The same silent rewrite the `\u` case was: `\q` used to print `q`.
    assert_refused_at("say \"\\q\"\n", "escape");
}

#[test]
fn edge_a_refused_escape_is_refused_on_the_bytecode_path_too() {
    // `rb compile` runs the same lexer, so the refusal cannot be a
    // tree-walking-only diagnostic.
    assert!(
        compile_source("say \"\\uZZZZ\"\n").is_err(),
        "the compiler must refuse it too"
    );
    assert!(
        compile_source("say \"\\ud83d\"\n").is_err(),
        "the compiler must refuse it too"
    );
}

#[test]
fn edge_the_refusal_names_the_line_it_is_on() {
    let error = lex_error("set a to 1\nset b to 2\nsay \"\\uZZZZ\"\n");
    let rendered = format!("{error}");
    assert!(
        rendered.contains("\n  --> 3:"),
        "the escape is on line 3, reported {rendered:?}"
    );
}
