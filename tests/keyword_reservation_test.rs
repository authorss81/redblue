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

/// The message and position a parse failed at, or `None` when it parsed.
fn parse_failure(source: &str) -> Option<(String, (usize, usize))> {
    let error = Lexer::tokenize(source)
        .ok()
        .and_then(|tokens| redblue::parser::parse(tokens).err())?;

    let position = error.span().map(|span| (span.line, span.column));
    Some((
        error.to_string(),
        position.expect("a parser error carries a span"),
    ))
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

/// One snippet per reserved word, each putting the word in the position its own
/// grammar gives it. The scan above proves a `TokenKind::<Kind>` string appears
/// somewhere in `src/parser.rs`, which is evidence about text, not about the
/// parser. This table is the behavioural half: every word below has to survive a
/// real `parse`, so a word that is reserved but unread shows up here as a
/// concrete error message instead of a silent string match.
const REACHABLE: &[(&str, &str)] = &[
    ("set", "set total to 1\n"),
    ("constant", "constant answer to 42\n"),
    ("to", "set total to 1\n"),
    ("is", "set same to 1 is 1\n"),
    ("if", "if 1 is 1 then\n    say \"a\"\nend\n"),
    ("then", "if 1 is 1 then\n    say \"a\"\nend\n"),
    (
        "else",
        "if 1 is 1 then\n    say \"a\"\nelse\n    say \"b\"\nend\n",
    ),
    ("end", "if 1 is 1 then\n    say \"a\"\nend\n"),
    ("unless", "unless 1 is 2 then\n    say \"a\"\nend\n"),
    ("for", "for each x in [1]\n    say x\nend\n"),
    ("each", "for each x in [1]\n    say x\nend\n"),
    ("in", "for each x in [1]\n    say x\nend\n"),
    ("from", "for each x from 1 to 2\n    say x\nend\n"),
    ("times", "repeat 3 times\n    say \"a\"\nend\n"),
    ("while", "while 1 is 2\n    say \"a\"\nend\n"),
    (
        "repeat",
        "repeat 3 times\n    say \"a\"\nend\n",
    ),
    // The row has to run as well as parse: a reserved word whose construct
    // parses but dies at run time is not reachable, and the reader only finds
    // out when they write the word themselves. `n` counts to three so `until`
    // terminates on the third turn instead of running to the loop guard.
    ("until", "set n to 0\nrepeat\n    set n to n + 1\nuntil n is 3\nsay n\n"),
    ("break", "for each x in [1]\n    break\nend\n"),
    ("skip", "for each x in [1]\n    skip\nend\n"),
    (
        "return",
        "to answer()\n    return 1\nend\n",
    ),
    ("give", "to answer()\n    give back 1\nend\n"),
    ("back", "to answer()\n    give back 1\nend\n"),
    (
        "might",
        "set data to might fail files.read(\"x\")\n",
    ),
    (
        "fail",
        "set data to might fail files.read(\"x\")\n",
    ),
    ("say", "say \"a\"\n"),
    ("print", "print \"a\"\n"),
    (
        "try",
        "try\n    say \"a\"\ncatch error\n    say \"b\"\nend\n",
    ),
    (
        "catch",
        "try\n    say \"a\"\ncatch error\n    say \"b\"\nend\n",
    ),
    (
        "finally",
        "try\n    say \"a\"\nfinally\n    say \"b\"\nend\n",
    ),
    ("and", "set both to 1 and 2\n"),
    ("or", "set either to 1 or 2\n"),
    ("not", "set negated to not 1\n"),
    ("mod", "set remainder to 5 mod 2\n"),
    ("nothing", "set empty to nothing\n"),
    ("module", "module Helpers\n    say \"a\"\nend\n"),
    ("import", "import files\n"),
    (
        "export",
        "module Helpers\n    to helper()\n        say \"a\"\n    end\n    export helper\nend\n",
    ),
    ("as", "import files as f\n"),
    ("test", "test \"a case\"\n    say \"a\"\nend\n"),
    (
        "expect",
        "test \"a case\"\n    expect 1 to be 1\nend\n",
    ),
    (
        "object",
        "object Counter\n    has count\nend\n",
    ),
    (
        "has",
        "object Counter\n    has count\nend\n",
    ),
    // A field is only in scope through `this`, so the `can` row writes it the
    // way the row above does; the two differ only in the word each is filed
    // under, and both spell the word they are filed under.
    (
        "can",
        "object Counter\n    has count\n    to can bump()\n        set this.count to this.count + 1\n    end\nend\n",
    ),
    (
        "this",
        "object Counter\n    has count\n    to can bump()\n        set this.count to this.count + 1\n    end\nend\n",
    ),
    // The parent has to exist in the same file, or the analyzer refuses the
    // declaration at run time with `Unknown parent object`.
    (
        "extends",
        "object Base\n    has n\nend\nobject Tally extends Base\n    has total\nend\n",
    ),
    // `yes` and `no` are reserved in their own right and have one kind between
    // them, so their row carries the literal the parser writes.
    ("yes", "set flag to yes\n"),
    ("no", "set flag to no\n"),
];

/// Whether `source` contains `word` as a whole word, which is what a row of
/// `REACHABLE` has to do: a snippet that never spells the word it is filed
/// under proves nothing about that word, and the row can go stale — or be
/// copied from its neighbour — without any test noticing.
fn contains_word(source: &str, word: &str) -> bool {
    source
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .any(|piece| piece == word)
}

#[test]
fn edge_every_reserved_keyword_parses_in_the_position_its_grammar_gives_it() {
    let covered: std::collections::BTreeSet<&str> =
        REACHABLE.iter().map(|(word, _)| *word).collect();
    let missing: Vec<&str> = KEYWORDS
        .iter()
        .map(|(word, _)| *word)
        .filter(|word| !covered.contains(word))
        .collect();
    assert!(
        missing.is_empty(),
        "the reachability table does not cover every reserved word: {}. Add the word \
         with the snippet that exercises it, so no reserved word ships unproven",
        missing.join(", ")
    );

    for (word, source) in REACHABLE {
        // The row has to spell the word it is filed under. A snippet that never
        // writes it exercises whatever else it contains, and the word it is
        // meant to cover goes unproven.
        assert!(
            contains_word(source, word),
            "the reachability row for `{}` never writes that word, so it proves nothing \
             about it: {}",
            word,
            source
        );

        if let Some(message) = parse_error(source) {
            panic!(
                "`{}` is reserved but the parser cannot read it where it belongs: {}\n  in {}",
                word, message, source
            );
        }

        // And it has to run. A word whose construct parses and then dies at run
        // time is reserved-but-unreachable all the same, and the reader finds
        // out only by writing the word. `run` lexes, parses, analyzes and
        // executes in one go, so this is the whole pipeline, not a parse.
        if let Err(error) = run(source) {
            panic!(
                "`{}` is reserved and parses, but the program it builds does not run: {}\n  in {}",
                word, error, source
            );
        }
    }
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

/// The exact text a reader sees, so the message cannot drift back to a
/// `TokenKind` name one edit away without this going red.
///
/// The finding this phase fixes was a rejection reading `Unexpected token Async`
/// — a name from inside the implementation, at a position the writer could not
/// act on. Both halves are pinned: the word, and the column that points at it.
/// `set if to 1` puts `if` at 1-based column 5.
#[test]
fn edge_the_rejection_of_a_reserved_word_names_the_word_at_its_column() {
    let (message, position) = parse_failure("set if to 1\n")
        .expect("`set if to 1` must be refused because `if` is a keyword");

    assert!(
        message.contains("Expected variable name, found reserved word 'if'"),
        "the rejection must name the word, got: {}",
        message
    );
    assert!(
        !message.contains("TokenKind::"),
        "the rejection names an internal type instead of the word: {}",
        message
    );
    assert_eq!(
        position,
        (1, 5),
        "the rejection must point at the offending word's column"
    );

    // The same holds for a word that reaches the expression reader with no rule
    // for it: `and` alone is a rejection, and it names `and`.
    let (message, position) =
        parse_failure("and 1\n").expect("a bare `and` is not an expression and must be refused");
    assert!(
        message.contains("Unexpected token 'and'"),
        "the rejection must name the word, got: {}",
        message
    );
    assert_eq!(position, (1, 1), "the rejection must point at column 1");
}

/// The name positions that ask for a name of their own rather than sharing the
/// generic one: an `object`'s own name, the name of the object it `extends`, and
/// a `test`'s name.
///
/// These three read their names by hand and were the last three sites still
/// reporting a bare `Expected object name` / `Expected parent object name` /
/// `Expected test name`, which names what was wanted and not what was found —
/// the reader is left looking for a missing word rather than at the reserved one
/// in front of them.
#[test]
fn edge_a_reserved_word_where_an_object_or_test_name_is_required_is_refused_by_name() {
    for (source, expected, position) in [
        (
            "object if\n    has a\nend\n",
            "Expected object name, found reserved word 'if'",
            (1, 8),
        ),
        (
            "object Base\n    has a\nend\nobject Child extends end\n",
            "Expected parent object name, found reserved word 'end'",
            (4, 22),
        ),
        (
            "test end\n    say \"a\"\nend\n",
            "Expected test name, found reserved word 'end'",
            (1, 6),
        ),
    ] {
        let (message, at) =
            parse_failure(source).unwrap_or_else(|| panic!("`{}` must be refused", source.trim()));

        assert!(
            message.contains(expected),
            "the rejection of `{}` must read `{}`, got: {}",
            source.trim(),
            expected,
            message
        );
        assert!(
            !message.contains("TokenKind::"),
            "the rejection names an internal type instead of the word: {}",
            message
        );
        assert_eq!(
            at, position,
            "the rejection must point at the offending word's column"
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
    // Each rejection has to carry a span — the reader has to be able to find the
    // word — and has to name the word rather than a `TokenKind` from inside the
    // implementation. `parse_failure` panics on a parser error without a span,
    // so the span is asserted by being able to reach the column at all.
    for (word, source, column) in [
        ("if", "set if to 1\n", 5),
        ("if", "set r to { if: 1 }\n", 12),
        ("if", "set f to (if) give back 1\n", 11),
        ("if", "import if\n", 8),
    ] {
        let (message, (line, at)) = parse_failure(source)
            .unwrap_or_else(|| panic!("`{}` should be refused", source.trim()));

        assert!(
            message.contains(word),
            "`{}` in `{}` should name the offending word: {}",
            word,
            source.trim(),
            message
        );
        assert!(
            !message.contains("TokenKind::"),
            "`{}` in `{}` names an internal type instead of the word: {}",
            word,
            source.trim(),
            message
        );
        assert_eq!(
            (line, at),
            (1, column),
            "`{}` should be refused with a span pointing at the offending word",
            source.trim()
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

    // § Keywords is the first table a reader meets, and it has to agree with the
    // appendix. A freed word listed here is a promise the parser cannot keep,
    // whatever the appendix says three hundred lines later.
    let keywords_section = SPEC
        .split("### Keywords")
        .nth(1)
        .and_then(|rest| rest.split("\n## ").next())
        .expect("SPEC.md must keep a Keywords section under the lexical structure");
    let listed = listed_keyword_words(keywords_section);
    for word in FREED {
        assert!(
            !listed.iter().any(|listed| listed == word),
            "SPEC.md § Keywords lists `{}` as a keyword; it is an ordinary identifier \
             since this phase",
            word
        );
    }

    // Neither the async forms nor object instantiation may be written as working
    // syntax anywhere but under a Not-implemented note. `wait` and `new` are
    // names a program may bind, so `wait network.get(...)` and `new Person` are
    // two expressions in a row and a parse error.
    for (name, document) in [("SPEC.md", SPEC), ("docs/GRAMMAR.md", GRAMMAR)] {
        for promise in ["wait network.get", "wait network.post", "new Person"] {
            for section in sections_containing(document, promise) {
                assert!(
                    section.to_lowercase().contains("not implemented"),
                    "{} writes `{}` in `{}`, which does not say the form is Not \
                     implemented; `wait` and `new` are ordinary identifiers, so the form \
                     is a parse error rather than syntax",
                    name,
                    promise,
                    heading_of(&section)
                );
            }
        }
    }
}

/// The keyword words a Markdown table in `section` lists, split on every
/// separator a table row uses, so a cell written `set, constant, to` or
/// `if/then/else/end` yields its words and not the cell.
fn listed_keyword_words(section: &str) -> Vec<String> {
    section
        .lines()
        .filter(|line| line.starts_with('|'))
        .flat_map(|line| line.trim_matches('|').split('|'))
        .flat_map(|cell| cell.split([',', '/', ' ', '`']))
        .map(|word| word.trim().to_lowercase())
        .filter(|word| !word.is_empty() && !word.chars().all(|c| c == '-'))
        .collect()
}

/// Every section of a Markdown document: the text from one heading to the next,
/// heading of either level.
fn sections_of(document: &str) -> Vec<String> {
    let mut sections = Vec::new();
    let mut current = String::new();

    for line in document.lines() {
        if line.starts_with('#') {
            sections.push(std::mem::take(&mut current));
        }
        current.push_str(line);
        current.push('\n');
    }
    sections.push(current);

    sections
}

/// The sections of `document` that mention `needle`.
fn sections_containing(document: &str, needle: &str) -> Vec<String> {
    sections_of(document)
        .into_iter()
        .filter(|section| section.contains(needle))
        .collect()
}

/// The heading line of a section, for a message that has to say where it looked.
fn heading_of(section: &str) -> &str {
    section
        .lines()
        .find(|line| line.starts_with('#'))
        .unwrap_or("<no heading>")
}

#[test]
fn a_freed_word_is_only_spelled_as_a_keyword_under_a_not_implemented_note() {
    // A freed word may still appear in the grammar as the shape a future
    // implementation has to match — `when_statement`, `async_call`,
    // `parallel_block`, `object_literal` all do. What it may not do is sit in a
    // section that reads as live grammar, because a reader writes the production
    // down and the program then dies in the parser. Every section that spells one
    // has to say out loud that nothing in it parses today.
    for word in FREED {
        for section in sections_containing(GRAMMAR, &format!("'{}'", word)) {
            assert!(
                section.to_lowercase().contains("not implemented"),
                "docs/GRAMMAR.md spells `'{}'` as a keyword in `{}`, which does not say the \
                 form is Not implemented; a freed word is an identifier, so a production that \
                 reads it is a promise the parser cannot keep",
                word,
                heading_of(&section)
            );
        }
    }
}
