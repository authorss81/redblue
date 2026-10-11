//! Every word the lexer reserves must be a word the parser can read.
//!
//! The lexer's keyword table and the parser's `match` arms are two halves of one
//! contract, and nothing in either file checked the other. Nine words sat in the
//! table with no arm anywhere in `src/parser.rs` to match them, so the lexer
//! produced a token the parser could not consume: the program died with
//! `ParserError: Unexpected token Async`, naming an internal `TokenKind` variant
//! the reader has never seen, and the word could not be used as an ordinary
//! identifier either (`set async to 1` died the same way).
//!
//! The first test here is the enumeration that would have caught it. It reads
//! `src/parser.rs` as text and requires a `TokenKind::<Kind>` arm for every
//! keyword the lexer reserves, so adding a word to `KEYWORDS` without giving the
//! parser a rule for it fails this test rather than shipping.

use redblue::lexer::{Lexer, TokenKind, KEYWORDS};
use redblue::Error;

const PARSER_SOURCE: &str = include_str!("../src/parser.rs");
const SPEC: &str = include_str!("../SPEC.md");
const GRAMMAR: &str = include_str!("../docs/GRAMMAR.md");

/// The two-word forms the table spells as one keyword.
const TWO_WORD: &[&str] = &["give back", "might fail"];

/// The words this phase freed: the lexer reserved them and no parser arm read
/// them, so they are identifiers again.
const FREED: &[&str] = &[
    "are", "ask", "async", "done", "new", "parallel", "that", "wait", "when",
];

/// The kind name as written in the `TokenKind` enum, without the payload a
/// `Debug` print puts on a value-carrying variant: the parser writes
/// `TokenKind::YesNo(_)`, not `TokenKind::YesNo(true)`.
fn kind_name(kind: &TokenKind) -> String {
    let printed = format!("{:?}", kind);
    match printed.find('(') {
        Some(open) => printed[..open].to_string(),
        None => printed,
    }
}

/// Runs a program through the full pipeline and returns what it printed, or the
/// error it reported.
fn run(source: &str) -> Result<Vec<String>, Error> {
    let tokens = Lexer::tokenize(source)?;
    let ast = redblue::parser::parse(tokens)?;
    redblue::analyzer::analyze(&ast)?;
    let (mut vm, result) = redblue::run_isolated(&ast);
    result.map(|_| vm.take_output())
}

/// The parser's complaint about a token, or `None` when the token parsed.
fn parse_error(source: &str) -> Option<String> {
    Lexer::tokenize(source)
        .ok()
        .and_then(|tokens| redblue::parser::parse(tokens).err())
        .map(|error| error.to_string())
}

// ---------------------------------------------------------------------------
// The enumeration: no keyword without a parser arm
// ---------------------------------------------------------------------------

#[test]
fn every_reserved_keyword_has_a_parser_arm() {
    let mut orphans = Vec::new();

    for (word, kind) in KEYWORDS {
        // `YesNo(true)` and friends carry a payload; the arm is written as
        // `TokenKind::YesNo(_)`, which the bare name still matches.
        let arm = format!("TokenKind::{}", kind_name(kind));
        if !PARSER_SOURCE.contains(&arm) {
            orphans.push(format!("`{}` ({})", word, arm));
        }
    }

    assert!(
        orphans.is_empty(),
        "the lexer reserves {} word(s) no parser arm can read: {}. Either give the \
         parser a rule for them, or free the words so they lex as identifiers",
        orphans.len(),
        orphans.join(", ")
    );
}

#[test]
fn the_freed_words_lex_as_identifiers_and_run_as_names() {
    for word in FREED {
        assert!(
            !Lexer::keywords().contains(word),
            "`{}` is still reserved by the lexer",
            word
        );

        let source = format!("set {} to 1\nsay {}\n", word, word);
        let output = run(&source)
            .unwrap_or_else(|error| panic!("`{}` should be an ordinary name: {}", word, error));

        assert_eq!(
            output,
            vec!["1".to_string()],
            "`set {} to 1` then `say {}` printed the wrong thing",
            word,
            word
        );
    }
}

// ---------------------------------------------------------------------------
// Error messages name the word, not the token kind
// ---------------------------------------------------------------------------

/// Words the parser really does own: a rejection here is a correct one, and it
/// has to say which word it rejected. Each is one that reaches the expression
/// reader with no rule for it, which is the path that used to name a `TokenKind`.
const RESERVED: &[&str] = &[
    "and", "as", "can", "catch", "each", "else", "extends", "finally", "from", "has", "in", "or",
    "then", "times",
];

/// Words the parser owns and that can stand where a name is required.
const NAME_POSITION: &[&str] = &["if", "end", "say", "from", "each", "while"];

#[test]
fn a_rejected_keyword_names_the_word_not_the_token_kind() {
    for word in RESERVED {
        let message = parse_error(&format!("{} 1\n", word))
            .unwrap_or_else(|| panic!("`{}` should not start an expression", word));

        assert!(
            message.contains(word),
            "the rejection of `{}` does not name the word: {}",
            word,
            message
        );
        assert!(
            !message.contains("TokenKind::"),
            "the rejection of `{}` names an internal type instead of the word: {}",
            word,
            message
        );
    }
}

#[test]
fn a_reserved_word_as_a_variable_name_is_refused_by_name() {
    for word in NAME_POSITION {
        let message = parse_error(&format!("set {} to 1\n", word))
            .unwrap_or_else(|| panic!("`set {} to 1` should be refused", word));

        assert!(
            message.contains(word),
            "`set {} to 1` should name the offending word: {}",
            word,
            message
        );
    }
}

// ---------------------------------------------------------------------------
// Edge cases: the four positions a reserved word shows up in
// ---------------------------------------------------------------------------

#[test]
fn edge_reserved_word_in_each_name_position() {
    // A freed word is an ordinary name in all four positions: a `set` target, a
    // record key, a function parameter and a module name.
    let source = "\
to twice(done)
    give back done * 2
end
set parallel to 2
set ask to twice(parallel)
set that to ask
say that
";
    let output = run(source)
        .unwrap_or_else(|error| panic!("freed words should be ordinary names: {}", error));

    assert_eq!(
        output,
        vec!["4".to_string()],
        "a freed word did not survive as a parameter and a local name"
    );

    // A word the parser owns is still refused, still by name, in each position.
    for (word, source) in [
        ("if", "set if to 1\n"),
        ("if", "set r to { if: 1 }\n"),
        ("if", "set f to (if) give back 1\n"),
        ("if", "import if\n"),
    ] {
        let message = parse_error(source)
            .unwrap_or_else(|| panic!("`{}` in `{}` should be refused", word, source.trim()));

        assert!(
            message.contains(word),
            "`{}` in `{}` should name the offending word: {}",
            word,
            source.trim(),
            message
        );
    }
}

#[test]
fn edge_freed_word_keeps_working_inside_records_lists_and_fields() {
    // The point of freeing a word is that it stops being special everywhere, not
    // only in a `set`. A record key, a list element, a field read and a string
    // all have to treat it as an ordinary name.
    let source = "\
set row to { new: \"a\", that: \"b\" }
set names to [\"when\", \"done\", \"wait\"]
say row.new
say row.that
say names[0]
say names[2]
";
    let output =
        run(source).unwrap_or_else(|error| panic!("freed words should work everywhere: {}", error));

    assert_eq!(
        output,
        vec![
            "a".to_string(),
            "b".to_string(),
            "when".to_string(),
            "wait".to_string(),
        ],
        "a freed word did not survive as a record key, a list element or a field"
    );
}

#[test]
fn edge_freed_word_is_still_an_ordinary_binding() {
    // A freed word reads as an identifier everywhere an identifier reads, which
    // includes being rebound and being read before it is set only where the
    // analyzer would have complained about any other name.
    let source = "\
set when to 1
set when to when + 1
for each done in [1, 2]
    set when to when + done
end
say when
";
    let output = run(source)
        .unwrap_or_else(|error| panic!("a freed word should bind and rebind: {}", error));

    assert_eq!(
        output,
        vec!["5".to_string()],
        "a freed word did not behave as an ordinary binding"
    );
}

#[test]
fn edge_a_word_is_not_reserved_by_accident_of_being_a_prefix() {
    // Freeing is by whole word: `async` is an identifier, `async_task` is a
    // different identifier, and neither takes the other's meaning.
    let source = "set async_task to 1\nset async to 2\nsay async_task\nsay async\n";
    let output = run(source)
        .unwrap_or_else(|error| panic!("a freed word must not shadow a prefix: {}", error));

    assert_eq!(
        output,
        vec!["1".to_string(), "2".to_string()],
        "a freed word collided with a longer name that starts with it"
    );

    // And the reverse: a reserved word is refused even when it is glued to
    // nothing at all, which is what the word-boundary match buys.
    assert!(
        Lexer::tokenize("if").unwrap()[0].kind != TokenKind::Identifier("if".to_string()),
        "`if` must still lex as a keyword"
    );
    assert_eq!(
        Lexer::tokenize("async").unwrap()[0].kind,
        TokenKind::Identifier("async".to_string()),
        "`async` must lex as an identifier"
    );
}

// ---------------------------------------------------------------------------
// The words other phases own
// ---------------------------------------------------------------------------

#[test]
fn words_owned_by_other_phases_still_fail_the_same_way() {
    // `until` belongs to phase-040 and `might fail` to phase-038. This phase
    // must not absorb them, and must not quietly change how they are refused
    // either: the word is still reserved, and a use the parser cannot read is
    // still a parser error.
    for (word, kind) in [
        ("until", "Until"),
        ("might", "MightFail"),
        ("fail", "MightFail"),
    ] {
        assert!(
            Lexer::keywords().contains(&word),
            "`{}` must stay reserved for its owning phase",
            word
        );
        assert!(
            parse_error(&format!("{}\n", word)).is_some(),
            "`{}` is reserved and still unread by the parser, so it must be refused",
            word
        );
        assert!(
            PARSER_SOURCE.contains(&format!("TokenKind::{}", kind)),
            "`{}` should be handled by phase-038/040, not by this phase",
            word
        );
    }
}

// ---------------------------------------------------------------------------
// The documentation says the same thing as the lexer
// ---------------------------------------------------------------------------

/// The words `docs/GRAMMAR.md` §1.3 lists, read out of the fenced block that
/// holds the table. The one-word forms only: the table also spells the two-word
/// keywords `give back` and `might fail`.
fn grammar_keywords() -> Vec<String> {
    let section = GRAMMAR
        .split("### 1.3 Keywords (Reserved)")
        .nth(1)
        .and_then(|rest| rest.split("```").nth(1))
        .expect("GRAMMAR.md must keep a fenced keyword table under §1.3");

    section
        .split('\'')
        .skip(1)
        .step_by(2)
        .filter(|word| !word.contains(' '))
        .map(|word| word.to_string())
        .collect()
}

#[test]
fn the_documented_keyword_set_is_the_lexers_keyword_set() {
    let documented = grammar_keywords();
    let reserved = Lexer::keywords();

    let listed_but_not_reserved: Vec<&str> = documented
        .iter()
        .filter(|word| !reserved.contains(&word.as_str()))
        .map(|word| word.as_str())
        .collect();
    assert!(
        listed_but_not_reserved.is_empty(),
        "docs/GRAMMAR.md §1.3 lists {} word(s) the lexer does not reserve, so a \
         program cannot use them: {}",
        listed_but_not_reserved.len(),
        listed_but_not_reserved.join(", ")
    );

    let reserved_but_not_listed: Vec<&str> = reserved
        .iter()
        .filter(|word| !documented.contains(&word.to_string()))
        .copied()
        .collect();
    assert!(
        reserved_but_not_listed.is_empty(),
        "the lexer reserves {} word(s) docs/GRAMMAR.md §1.3 does not list: {}",
        reserved_but_not_listed.len(),
        reserved_but_not_listed.join(", ")
    );
}

#[test]
fn spec_no_longer_promises_the_forms_this_phase_freed() {
    // SPEC.md documented `async`, `wait`, `parallel` and `done` as working
    // syntax. They are identifiers now, and the section says so where a reader
    // of the async section will look, not only in the appendix at the end.
    let asynchronous = SPEC
        .split("## Asynchronous Programming")
        .nth(1)
        .and_then(|rest| rest.split("\n## ").next())
        .expect("SPEC.md must keep an Asynchronous Programming section");

    assert!(
        asynchronous.contains("Not implemented"),
        "the Asynchronous Programming section must say the form is not implemented"
    );

    for word in FREED {
        assert!(
            !SPEC.contains(&format!("| {} |", word)),
            "SPEC.md's keyword appendix still lists `{}` as a keyword",
            word
        );
    }

    // The two-word forms the lexer does spell as one keyword stay documented.
    for phrase in TWO_WORD {
        assert!(
            SPEC.contains(phrase),
            "SPEC.md should still document `{}`",
            phrase
        );
        assert!(
            GRAMMAR.contains(phrase),
            "docs/GRAMMAR.md should still document `{}`",
            phrase
        );
    }
}
