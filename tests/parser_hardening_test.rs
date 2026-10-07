use redblue::lexer::Lexer;
use redblue::parser::{parse, Parser, MAX_BLOCK_DEPTH, MAX_NESTING_DEPTH};
use redblue::Error;

fn lex_and_parse(source: &str) -> Result<redblue::parser::Program, Error> {
    let tokens = Lexer::tokenize(source)?;
    parse(tokens)
}

fn parser_error(source: &str) -> Error {
    lex_and_parse(source).expect_err("source should fail to parse")
}

/// Builds `set x to ` followed by `open` repeated `depth` times, `1`, and
/// `close` repeated `depth` times.
fn nested(open: &str, close: &str, depth: usize) -> String {
    format!(
        "set x to {}{}{}\n",
        open.repeat(depth),
        "1",
        close.repeat(depth)
    )
}

fn assert_spanned_parser_error(error: &Error) {
    match error {
        Error::Parser(message, span) => {
            assert!(
                span.is_known(),
                "parser error must carry a span, got {span:?} for {message:?}"
            );
            assert!(!message.is_empty(), "parser error must carry a message");
        }
        other => panic!("expected a Parser error, got {other:?}"),
    }
}

// --- empty / degenerate input ------------------------------------------------

#[test]
fn edge_empty_file_parses_to_empty_program() {
    let program = lex_and_parse("").expect("an empty file is valid Redblue");
    assert!(
        program.statements.is_empty(),
        "empty file should produce no statements, got {}",
        program.statements.len()
    );
}

#[test]
fn edge_whitespace_only_and_comment_only_are_empty_programs() {
    for source in [
        "   ",
        "\n\n\n",
        "\t \t",
        " \r\n \r\n",
        "// only a comment\n",
    ] {
        let program =
            lex_and_parse(source).unwrap_or_else(|e| panic!("{source:?} should parse: {e}"));
        assert!(
            program.statements.is_empty(),
            "{source:?} should produce no statements, got {}",
            program.statements.len()
        );
    }
}

// --- malformed input --------------------------------------------------------

#[test]
fn edge_unclosed_constructs_produce_spanned_parser_errors() {
    let cases = [
        ("if true then\n  say \"hi\"\n", "unclosed `end`"),
        ("to f()\n  say 1\n", "unclosed function"),
        ("for each x in [1, 2]\n  say x\n", "unclosed for"),
        ("set x to [1, 2, 3\n", "unclosed bracket"),
        ("set x to (1 + 2\n", "unclosed paren"),
        ("set x to {a: 1\n", "unclosed brace"),
        ("say 1 +", "stray operator"),
        ("say 1 @ 2\n", "stray character (lexer)"),
    ];

    for (source, label) in cases {
        match lex_and_parse(source) {
            Ok(_) => panic!("{label} ({source:?}) should not parse"),
            Err(error) => assert!(
                error.span().is_some(),
                "{label} ({source:?}) must carry a span, got {error:?}"
            ),
        }
    }
}

#[test]
fn edge_unclosed_string_is_a_spanned_lexer_error() {
    match Lexer::tokenize("say \"unterminated\n") {
        Ok(_) => panic!("unterminated string should not lex"),
        Err(Error::Lexer(message, span)) => {
            assert!(span.is_known(), "unterminated string needs a span");
            assert!(
                message.contains("Unterminated string"),
                "unexpected message {message:?}"
            );
        }
        Err(other) => panic!("expected a Lexer error, got {other:?}"),
    }
}

#[test]
fn edge_deep_nesting_is_a_clean_error_not_a_stack_overflow() {
    // Each of these used to recurse once per token with no bound, and aborted
    // the process with "has overflowed its stack" (SIGABRT) rather than
    // returning an error.
    let cases = [
        ("[", "]", "nested list literal"),
        ("(", ")", "nested parenthesis"),
        ("{a: ", "}", "nested record literal"),
    ];

    for (open, close, label) in cases {
        let source = nested(open, close, MAX_NESTING_DEPTH + 1);
        match lex_and_parse(&source) {
            Ok(_) => panic!("{label} past the depth limit should not parse"),
            Err(error) => assert_spanned_parser_error(&error),
        }
    }
}

#[test]
fn edge_deep_unary_chain_is_a_clean_error_not_a_stack_overflow() {
    let source = format!("set x to {}\n", "-".repeat(MAX_NESTING_DEPTH + 1));
    match lex_and_parse(&source) {
        Ok(_) => panic!("unary chain past the depth limit should not parse"),
        Err(error) => assert_spanned_parser_error(&error),
    }
}

#[test]
fn edge_long_flat_binary_chain_is_a_clean_error_not_a_stack_overflow() {
    // `1 + 1 + 1 ...` builds a left spine one Expr deep per operator. Dropping
    // that tree used to recurse per level and abort the process.
    let operators = MAX_NESTING_DEPTH + 2;
    let source = format!("set x to {}\n", vec!["1"; operators].join(" + "));
    match lex_and_parse(&source) {
        Ok(_) => panic!("binary chain past the depth limit should not parse"),
        Err(error) => assert_spanned_parser_error(&error),
    }
}

#[test]
fn edge_1000_deep_nested_list_never_overflows_the_stack() {
    // The depth the phase requires to be safe. Past the limit this must be a
    // bounded, spanned error; it must never abort the process.
    let source = nested("[", "]", 1000);
    match lex_and_parse(&source) {
        Ok(program) => assert_eq!(program.statements.len(), 1),
        Err(error) => assert_spanned_parser_error(&error),
    }
}

#[test]
fn edge_pathological_deep_nesting_at_100k_levels_is_rejected_quickly() {
    let source = nested("[", "]", 100_000);
    let error = lex_and_parse(&source).expect_err("100k-deep nesting must be rejected");
    assert_spanned_parser_error(&error);
}

// --- boundary: at, just under, and just over the limit ------------------------

#[test]
fn nesting_at_the_depth_limit_is_accepted_and_one_past_is_rejected() {
    let at_limit = nested("[", "]", MAX_NESTING_DEPTH);
    let program = lex_and_parse(&at_limit).expect("nesting exactly at the limit must parse");
    assert_eq!(program.statements.len(), 1);

    let over_limit = nested("[", "]", MAX_NESTING_DEPTH + 1);
    let error = lex_and_parse(&over_limit).expect_err("one past the limit must be rejected");
    assert_spanned_parser_error(&error);
}

#[test]
fn nesting_limit_is_a_sane_small_number() {
    // Guards against someone raising the limit back into stack-overflow
    // territory. libstd gives spawned (test) threads a 2 MiB stack.
    assert!(
        (16..=96).contains(&MAX_NESTING_DEPTH),
        "MAX_NESTING_DEPTH must stay within a 2 MiB stack, got {MAX_NESTING_DEPTH}"
    );
    assert!(
        (16..=96).contains(&MAX_BLOCK_DEPTH),
        "MAX_BLOCK_DEPTH must stay within a 2 MiB stack, got {MAX_BLOCK_DEPTH}"
    );
}

// --- block nesting ----------------------------------------------------------

/// Builds `depth` nested `open ... end` blocks around a single `say 1`.
fn nested_blocks(open: &str, depth: usize) -> String {
    format!(
        "{}say 1\n{}",
        format!("{open}\n").repeat(depth),
        "end\n".repeat(depth)
    )
}

#[test]
fn edge_deeply_nested_blocks_are_a_clean_error_not_a_stack_overflow() {
    // A block body is parsed by recursing back into `parse_statement`, so
    // 5000 nested `if`s used to abort the process outright.
    //
    // `module` is one of these forms for the same reason: `parse_module` reads
    // its body by recursing too. It was missing here *and* missing from
    // `Parser::opens_block`, so nested modules spent no budget at all and a
    // file of them overflowed the stack instead of reporting anything.
    for open in [
        "if true then",
        "while true",
        "repeat 1 times",
        "object Deep",
        "try",
        "test \"deep\"",
        "module Deep",
        "to deep()",
    ] {
        let source = nested_blocks(open, MAX_BLOCK_DEPTH + 1);
        match lex_and_parse(&source) {
            Ok(_) => panic!("{open:?} past the block depth limit should not parse"),
            Err(error) => assert_spanned_parser_error(&error),
        }
    }
}

#[test]
fn edge_deeply_nested_functions_are_a_clean_error_not_a_stack_overflow() {
    let source = format!(
        "{}say 1\n{}",
        "to f()\n".repeat(MAX_BLOCK_DEPTH + 1),
        "end\n".repeat(MAX_BLOCK_DEPTH + 1)
    );
    match lex_and_parse(&source) {
        Ok(_) => panic!("nested functions past the block depth limit should not parse"),
        Err(error) => assert_spanned_parser_error(&error),
    }
}

#[test]
fn block_nesting_at_the_depth_limit_is_accepted_and_one_past_is_rejected() {
    let at_limit = nested_blocks("if true then", MAX_BLOCK_DEPTH);
    let program = lex_and_parse(&at_limit).expect("block nesting at the limit must parse");
    assert_eq!(program.statements.len(), 1);

    let over_limit = nested_blocks("if true then", MAX_BLOCK_DEPTH + 1);
    let error = lex_and_parse(&over_limit).expect_err("one past the limit must be rejected");
    assert_spanned_parser_error(&error);
}

#[test]
fn edge_5000_nested_blocks_never_overflow_the_stack() {
    // The input that aborted the process before the block guard existed.
    let source = nested_blocks("if true then", 5000);
    let error = lex_and_parse(&source).expect_err("5000 nested blocks must be rejected");
    assert_spanned_parser_error(&error);
}

#[test]
fn sibling_blocks_do_not_spend_each_others_budget() {
    // 100 sibling blocks in a row is 100 statements, not 100 levels of
    // nesting: a flat file of `if` must not hit the limit.
    let source = "if true then\n  say 1\nend\n".repeat(200);
    let program = lex_and_parse(&source).expect("sibling blocks must not nest");
    assert_eq!(program.statements.len(), 200);
}

// --- resource limit ---------------------------------------------------------

#[test]
fn token_stream_without_eof_terminates_instead_of_looping_forever() {
    // `Lexer::tokenize` always ends the stream with `Eof`, but `Parser::new`
    // is public and accepts any token list. Without an end token the statement
    // loop used to spin forever; this asserts it comes back.
    let tokens = Lexer::tokenize("set x to 1\nsay \"hi\"\n")
        .expect("source should lex")
        .into_iter()
        .filter(|token| token.kind != redblue::lexer::TokenKind::Eof)
        .collect();

    let program = Parser::new(tokens)
        .parse()
        .expect("a token stream with no Eof must still terminate");

    assert_eq!(program.statements.len(), 2);
}

#[test]
fn empty_token_stream_parses_to_an_empty_program() {
    let program = Parser::new(Vec::new()).parse().expect("no tokens is valid");
    assert!(program.statements.is_empty());
}

#[test]
fn malformed_input_error_names_what_it_expected() {
    let unclosed_end = parser_error("if true then\n  say \"hi\"\n");
    match &unclosed_end {
        Error::Parser(message, span) => {
            assert!(
                message.contains("Expected"),
                "unhelpful message {message:?}"
            );
            assert_eq!(span.line, 3, "the EOF span points past the body");
        }
        other => panic!("expected a Parser error, got {other:?}"),
    }

    let stray_operator = parser_error("say 1 +\n");
    match &stray_operator {
        Error::Parser(message, _) => assert!(
            message.contains("Unexpected token"),
            "unhelpful message {message:?}"
        ),
        other => panic!("expected a Parser error, got {other:?}"),
    }

    // A list that is never closed runs into the newline and reports the token
    // it found instead of the `]` it wanted; either wording is a real
    // diagnostic, an empty one would not be.
    let unclosed_bracket = parser_error("set x to [1, 2, 3\n");
    match &unclosed_bracket {
        Error::Parser(message, span) => {
            assert!(
                message.contains("Expected") || message.contains("Unexpected token"),
                "unhelpful message {message:?}"
            );
            assert!(span.is_known(), "unclosed bracket needs a span");
        }
        other => panic!("expected a Parser error, got {other:?}"),
    }
}

#[test]
fn type_mismatch_shaped_input_is_a_parser_error_not_a_panic() {
    // `set` with a non-identifier target, `for` without `each`, a record with
    // a non-identifier key, `import` with nothing after it.
    let cases = [
        "set 1 to 2\n",
        "for x in [1]\nend\n",
        "set x to {1: 2}\n",
        "import\n",
        "set to 5\n",
    ];

    for source in cases {
        match lex_and_parse(source) {
            Ok(_) => panic!("{source:?} should not parse"),
            Err(error) => assert!(
                error.span().is_some(),
                "{source:?} must carry a span, got {error:?}"
            ),
        }
    }
}

#[test]
fn edge_100k_token_flat_input_parses_linearly() {
    // ~100k tokens of independent statements. Time-boxed rather than asserted
    // for a specific duration so the check is not machine-dependent, but tight
    // enough to fail on quadratic behaviour.
    let statements = 25_000;
    let source = "set x to 1\n".repeat(statements);

    let started = std::time::Instant::now();
    let program = lex_and_parse(&source).expect("a flat 100k-token input must parse");
    let elapsed = started.elapsed();

    assert_eq!(program.statements.len(), statements);
    assert!(
        elapsed.as_secs() < 20,
        "parsing {statements} statements took {elapsed:?}, which is not linear"
    );
}

#[test]
fn edge_100k_token_flat_input_scales_sub_quadratically() {
    // 10x the tokens must cost far less than 100x the time. Asserted with a
    // generous 40x ceiling so a slow, noisy machine cannot make this flaky.
    fn time(source: &str) -> std::time::Duration {
        let started = std::time::Instant::now();
        lex_and_parse(source).expect("flat input must parse");
        started.elapsed()
    }

    let small = "set x to 1\n".repeat(2_000);
    let large = "set x to 1\n".repeat(20_000);

    time(&small); // warm up the allocator
    let small_time = time(&small).as_nanos().max(1);
    let large_time = time(&large).as_nanos().max(1);

    assert!(
        large_time < small_time * 40,
        "10x tokens took {large_time}ns vs {small_time}ns for 1x, which is not linear"
    );
}

// --- singleton / unicode -----------------------------------------------------

#[test]
fn edge_single_element_collections_and_unicode_survive() {
    let program = lex_and_parse(
        "set empty to []\nset single to [1]\nset empty_rec to {}\nset one to {a: 1}\nsay \"\u{1f600} \u{4f60}\u{597d}\"\n",
    )
    .expect("singleton and unicode literals must parse");
    assert_eq!(program.statements.len(), 5);
}
