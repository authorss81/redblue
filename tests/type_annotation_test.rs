//! The type system the grammar and the specification promise is not implemented,
//! and now the documents say so — and a test holds them to it.
//!
//! The finding: `docs/GRAMMAR.md` §2 carried a whole static type system
//! (`type_expression`, `primitive_type`, `list_type`, `record_type`,
//! `object_type`, `function_type`, `generic_type`, `union_type`) with no
//! disclaimer, and five productions elsewhere read that type expression back —
//! §4.1's `declaration` (`set` and `constant`), §4.5's `function_decl` and
//! `parameter`, §4.6's `property_decl` — while SPEC.md's "Complex Types"
//! showed `set numbers to list of number` as if it were a program. None of it
//! parses. `set x : number to 1` dies with `Expected 'to' but got Colon`,
//! `to f(a) needs number` dies with `Unknown variable 'needs'`, and `needs` and
//! `gives` are ordinary identifiers the keyword table does not list.
//!
//! The project already has the honest convention and had simply missed this
//! section: GRAMMAR.md §1.3 lists only words the parser can read, and §4.3's
//! `when_statement` and §4.8's async productions each carry a **Not
//! implemented** note. This file marks §2 and the five productions the same way
//! and pins the marking three ways:
//!
//! 1. every annotated form is *refused* — if a future phase implements the
//!    annotations, this test goes red and the note has to come off in the same
//!    change, rather than the documents lying for another phase;
//! 2. every production that carries the note really does carry it, in the shape
//!    §4.3 established (a `//` comment directly above the production);
//! 3. every word a marked production spells as a literal is either a reserved
//!    keyword or inside a marked production, so `needs` and `gives` can never
//!    creep back into the grammar as words nothing reads.
//!
//! The bare form of every one of the five constructs is run as well, because a
//! note that says "the annotation does not parse" is a lie if the *declaration*
//! stopped parsing with it.

use redblue::lexer::Lexer;
use redblue::Error;

const GRAMMAR: &str = include_str!("../docs/GRAMMAR.md");
const SPEC: &str = include_str!("../SPEC.md");

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Runs a program through the whole front end — lex, parse, analyze — and
/// returns the failure, or `None` when the program got all the way through the
/// front end and would have run.
///
/// The front end rather than the parser alone, because two of the forms this
/// file pins fail later than parsing: `to f(a) needs number` parses (the parser
/// reads `needs` as an identifier expression and `number` as the next one) and
/// is refused by the analyzer, and so is `set numbers to list of number`.
fn refused(source: &str) -> Option<Error> {
    let tokens = match Lexer::tokenize(source) {
        Ok(tokens) => tokens,
        Err(error) => return Some(error),
    };
    let ast = match redblue::parser::parse(tokens) {
        Ok(ast) => ast,
        Err(error) => return Some(error),
    };
    redblue::analyzer::analyze(&ast).err()
}

/// The failure `source` produced, or a panic explaining that it produced none.
fn expect_refusal(source: &str, what: &str) -> Error {
    refused(source).unwrap_or_else(|| {
        panic!(
            "{} is accepted by the front end today:\n{}\nEither the documents' \
             Not-implemented note has to come off and this test with it, or the \
             rejection has been lost.",
            what, source
        )
    })
}

/// Runs a program through the whole pipeline and returns what it printed.
fn run(source: &str) -> Vec<String> {
    let tokens = Lexer::tokenize(source).expect("the program should lex");
    let ast = redblue::parser::parse(tokens).expect("the program should parse");
    redblue::analyzer::analyze(&ast).expect("the program should analyze");
    let (mut vm, result) = redblue::run_isolated(&ast);
    result.expect("the program should run");
    vm.take_output()
}

// ---------------------------------------------------------------------------
// The five productions, and the state each is in
// ---------------------------------------------------------------------------

/// One construct that the grammar spells with an optional type annotation.
///
/// * `anchor` — a fragment of the grammar line carrying that annotation, used to
///   find the line the note has to sit on.
/// * `bare` — the same construct with the annotation left out, which must run.
/// * `printed` — what `bare` prints.
/// * `annotated` / `malformed` / `doubled` — the annotation present, present
///   with nothing after the colon, and written twice. All three must be
///   refused.
struct Host {
    name: &'static str,
    anchor: &'static str,
    bare: &'static str,
    printed: &'static [&'static str],
    annotated: &'static str,
    malformed: &'static str,
    doubled: &'static str,
}

const HOSTS: &[Host] = &[
    Host {
        name: "`set`",
        anchor: "'set' identifier [ ':' type_expression ]",
        bare: "set x to 1\nsay x\n",
        printed: &["1"],
        annotated: "set x : number to 1\nsay x\n",
        malformed: "set x : to 1\nsay x\n",
        doubled: "set x : number : text to 1\nsay x\n",
    },
    Host {
        name: "`constant`",
        anchor: "'constant' identifier [ ':' type_expression ]",
        bare: "constant C to 1\nsay C\n",
        printed: &["1"],
        annotated: "constant C : number to 1\nsay C\n",
        malformed: "constant C : to 1\nsay C\n",
        doubled: "constant C : number : text to 1\nsay C\n",
    },
    Host {
        name: "`parameter`",
        anchor: "parameter        = identifier [ ':' type_expression ]",
        bare: "to f(x)\n    give back x * 2\nend\nsay f(3)\n",
        printed: &["6"],
        annotated: "to f(x : number)\n    give back x * 2\nend\nsay f(3)\n",
        malformed: "to f(x :)\n    give back x * 2\nend\n",
        doubled: "to f(x : number : text)\n    give back x * 2\nend\n",
    },
    Host {
        name: "`has`",
        anchor: "property_decl    = 'has' identifier [ ':' type_expression ]",
        bare: "object P\n    has x\nend\nset P.x to 7\nsay P.x\n",
        printed: &["7"],
        annotated: "object P\n    has x : number\nend\nsay P.x\n",
        malformed: "object P\n    has x :\nend\nsay P.x\n",
        doubled: "object P\n    has x : number : text\nend\nsay P.x\n",
    },
    Host {
        name: "function declaration",
        anchor: "function_decl    = 'to' identifier",
        bare: "to f(a)\n    give back a + 1\nend\nsay f(1)\n",
        printed: &["2"],
        annotated: "to f(a)\n    needs number\n    give back a + 1\nend\nsay f(1)\n",
        malformed: "to f(a)\n    needs\n    give back a + 1\nend\nsay f(1)\n",
        doubled: "to f(a)\n    needs number : text\n    give back a + 1\nend\nsay f(1)\n",
    },
];

/// The lines of `docs/GRAMMAR.md`.
fn grammar_lines() -> Vec<&'static str> {
    GRAMMAR.lines().collect()
}

/// Whether the grammar line at `index` carries a **Not implemented** note in the
/// shape §4.3 established: a run of `//` comments immediately above it. A note
/// at the top of a fenced block — §2.1's, which covers all eight of its
/// productions — is found this way too, because the production it marks is the
/// line below the run.
fn note_governs(lines: &[&str], index: usize) -> bool {
    // Step back off the production's own line.
    let mut at = match index.checked_sub(1) {
        Some(at) => at,
        None => return false,
    };
    while lines[at].trim().is_empty() {
        at = match at.checked_sub(1) {
            Some(at) => at,
            None => return false,
        };
    }
    // Collect the run of `//` lines ending at `at`, if that is what is there.
    // Anything else means this line has no note of its own.
    if !lines[at].trim_start().starts_with("//") {
        return false;
    }
    let mut note = String::new();
    loop {
        if !lines[at].trim_start().starts_with("//") {
            break;
        }
        note.insert_str(0, lines[at]);
        if at == 0 {
            break;
        }
        at -= 1;
    }
    note.to_lowercase().contains("not implemented")
}

/// The note and the prose a reader sees about the production at `index`: the run
/// of `//` comments immediately above it, plus the Markdown section it sits in.
/// A word the grammar spells as a literal has to be named in one of the two —
/// a note that says "Not implemented" without saying which words are affected
/// leaves a reader thinking `needs` is a keyword.
fn note_text(lines: &[&str], index: usize) -> String {
    let mut text = String::new();
    for line in &lines[section_bounds(lines, index)] {
        text.push_str(line);
        text.push('\n');
    }
    if let Some(comment) = comment_run(lines, index) {
        text.push_str(&comment);
    }
    text
}

/// The lines of the Markdown section `index` is in: from the heading above it to
/// the next heading of any level.
fn section_bounds(lines: &[&str], index: usize) -> std::ops::Range<usize> {
    let start = (0..=index)
        .rev()
        .find(|at| lines[*at].starts_with('#'))
        .unwrap_or(0);
    let end = ((index + 1)..lines.len())
        .find(|at| lines[*at].starts_with('#'))
        .unwrap_or(lines.len());
    start..end
}

/// The run of `//` comment lines immediately above `index`, or `None` when there
/// is not one.
fn comment_run(lines: &[&str], index: usize) -> Option<String> {
    let mut at = index.checked_sub(1)?;
    while lines[at].trim().is_empty() {
        at = at.checked_sub(1)?;
    }
    if !lines[at].trim_start().starts_with("//") {
        return None;
    }
    let mut note = String::new();
    loop {
        if !lines[at].trim_start().starts_with("//") {
            break;
        }
        note.insert_str(0, lines[at]);
        if at == 0 {
            break;
        }
        at -= 1;
    }
    Some(note)
}

/// The index of the first grammar line containing `anchor`.
fn grammar_line(anchor: &str) -> usize {
    grammar_lines()
        .iter()
        .position(|line| line.contains(anchor))
        .unwrap_or_else(|| panic!("docs/GRAMMAR.md must keep the production `{}`", anchor))
}

// ---------------------------------------------------------------------------
// 1. Every annotated form is refused
// ---------------------------------------------------------------------------

#[test]
fn every_type_annotation_the_grammar_promises_is_refused() {
    for host in HOSTS {
        for (what, source) in [
            ("the annotated form", host.annotated),
            ("the malformed form", host.malformed),
            ("the doubled form", host.doubled),
        ] {
            let error = expect_refusal(source, &format!("{} on {}", host.name, what));
            assert!(
                !error.to_string().is_empty(),
                "the rejection of {} on {} carries no message",
                what,
                host.name
            );
            assert!(
                error.span().is_some(),
                "the rejection of {} on {} must carry a position, got: {}",
                what,
                host.name,
                error
            );
        }
    }
}

/// The refusals are refusals a reader can act on, so the message names what was
/// wanted rather than an internal token kind, and the position points at the
/// annotation. The exact text is pinned so it cannot drift back to `Expected
/// RightParen but got Identifier("of")` — a `TokenKind` from inside the
/// implementation — one edit away without this going red.
#[test]
fn the_rejection_of_an_annotation_names_what_it_wanted() {
    for (source, message, (line, column)) in [
        (
            "set x : number to 1\n",
            "Expected 'to' but got Colon",
            (1, 7),
        ),
        (
            "constant C : number to 1\n",
            "Expected 'to' but got Colon",
            (1, 12),
        ),
        (
            "object P\n    has x : number\nend\n",
            "Unexpected token Colon",
            (2, 11),
        ),
        (
            "to f(x : number)\n    give back x\nend\n",
            "Expected RightParen but got Colon",
            (1, 8),
        ),
        // The `as` spelling SPEC.md's "Type Annotations" shows. `as` is
        // reserved — for `import ... as ...` — so the rejection has to name the
        // word rather than a `TokenKind`, which is what the assertion below
        // turns on.
        (
            "to add(a as number)\n    give back a\nend\n",
            "Expected RightParen but got 'as'",
            (1, 10),
        ),
    ] {
        let error = expect_refusal(source, &format!("the annotation in {:?}", source.trim()));
        let rendered = error.to_string();

        assert!(
            rendered.contains(message),
            "the rejection of `{}` must read `{}`, got: {}",
            source.trim(),
            message,
            rendered
        );
        assert!(
            !rendered.contains("TokenKind::"),
            "the rejection of `{}` names an internal type instead of the word: {}",
            source.trim(),
            rendered
        );

        let span = error.span().expect("a refusal with a position");
        assert_eq!(
            (span.line, span.column),
            (line, column),
            "the rejection of `{}` must point at the annotation's column",
            source.trim()
        );
    }
}

/// The two forms that get past the parser are refused by the analyzer, and the
/// analyzer's own message is what a reader sees — so the words it names are
/// pinned too. `needs` and `list` are ordinary identifiers, which is the whole
/// point: the grammar spells them as keywords in a section that now says they
/// are not keywords.
#[test]
fn edge_needs_gives_and_list_of_are_ordinary_identifiers_the_analyzer_names() {
    for (source, named) in [
        ("to f(a)\n    needs number\n    give back a\nend\n", "needs"),
        ("to f(a)\n    gives number\n    give back a\nend\n", "gives"),
        ("set numbers to list of number\n", "list"),
    ] {
        let error = expect_refusal(source, &format!("`{}`", source.trim()));
        let rendered = error.to_string();

        assert!(
            rendered.contains(&format!("Unknown variable '{}'", named)),
            "`{}` must be refused by naming `{}` as the unknown name, got: {}",
            source.trim(),
            named,
            rendered
        );
        assert!(
            error.span().is_some(),
            "the refusal of `{}` must carry a position",
            source.trim()
        );
    }

    // Being ordinary identifiers is not only a rejection: each one is a name a
    // program may bind, and a program that binds them still runs and prints.
    for (word, printed) in [
        ("needs", "1"),
        ("gives", "2"),
        ("list", "3"),
        ("record", "4"),
        ("number", "5"),
        ("text", "6"),
    ] {
        let source = format!("set {} to {}\nsay {}\n", word, printed, word);
        assert_eq!(
            run(&source),
            vec![printed.to_string()],
            "`{}` is not a keyword, so binding and reading it must work as a name",
            word
        );
    }
}

// ---------------------------------------------------------------------------
// 2. The five productions and §2 carry the note
// ---------------------------------------------------------------------------

#[test]
fn every_annotated_production_carries_a_not_implemented_note() {
    let lines = grammar_lines();

    for host in HOSTS {
        let index = grammar_line(host.anchor);
        assert!(
            note_governs(&lines, index),
            "docs/GRAMMAR.md's `{}` production for {} promises a type annotation the parser \
             cannot read, and carries no Not-implemented note above it. Add a `//` comment \
             directly above the production, as §4.3's `when_statement` does.",
            host.anchor,
            host.name
        );
    }
}

#[test]
fn the_type_expression_block_carries_a_not_implemented_note() {
    let lines = grammar_lines();
    let index = grammar_line("type_expression  = primitive_type");

    assert!(
        note_governs(&lines, index),
        "docs/GRAMMAR.md §2.1 spells a whole static type system with no Not-implemented note. \
         Add one at the top of the block, as §4.8 does."
    );

    // The section heading itself is what a reader lands on when they search for
    // "types", so the note has to be reachable from the chapter and not only
    // from inside the code block — where §4.3 puts `when`'s.
    let chapter = chapter_containing(GRAMMAR, "## 2. Type System");
    assert!(
        chapter.to_lowercase().contains("not implemented"),
        "docs/GRAMMAR.md §2 carries no Not-implemented note outside its code block:\n{}",
        heading_of(&chapter)
    );
}

// ---------------------------------------------------------------------------
// 3. Every keyword literal in a marked production is marked
// ---------------------------------------------------------------------------

/// The words `docs/GRAMMAR.md` §1.3 lists, read out of the fenced table.
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

/// The lines a note at `index` governs: the note's own run, the production, and
/// its continuation lines — the ones that do not start a new production, which
/// in this grammar is every line up to the blank line that ends it.
///
/// `whole_block` is §2.1's case, where one note at the top of the fenced block
/// covers all eight of the productions below it. It is not inferred from the
/// shape, because §4.1's note sits in exactly the same place and covers one
/// production: saying that the multiple-assignment `declaration` — which parses —
/// is Not implemented would be a second lie in the document this phase exists to
/// remove.
fn marked_region(lines: &[&str], index: usize, whole_block: bool) -> Vec<usize> {
    if whole_block {
        // The fence above the note, and the fence that closes it.
        let mut start = index;
        while start > 0 && !lines[start].trim_start().starts_with("//") {
            start -= 1;
        }
        let mut end = start;
        while end < lines.len() && lines[end].trim() != "```" {
            end += 1;
        }
        return (start..end).collect();
    }

    let mut start = index;
    while start > 0 && lines[start - 1].trim_start().starts_with("//") {
        start -= 1;
    }
    let mut end = index;
    while end < lines.len() && !lines[end].trim().is_empty() {
        end += 1;
    }
    (start..end).collect()
}

#[test]
fn every_word_a_marked_production_spells_as_a_literal_is_reserved_or_named_in_its_note() {
    let lines = grammar_lines();
    let reserved = grammar_keywords();

    // The productions this phase owns: §2.1's block and the five annotation
    // productions. Nothing else in the grammar is in scope — a word in §5 that
    // the parser does not read is a different phase's finding, and sweeping the
    // whole document here would turn this phase into a documentation rewrite.
    let types = grammar_line("type_expression  = primitive_type");
    let mut regions: Vec<(usize, bool)> = vec![(types, true)];
    for host in HOSTS {
        regions.push((grammar_line(host.anchor), false));
    }

    // Every region has to be marked, or the scan below proves nothing: an
    // unmarked production is the finding, not an exemption from it.
    let unmarked: Vec<&str> = regions
        .iter()
        .filter(|(index, _)| !note_governs(&lines, *index))
        .map(|_| "one of the type productions")
        .collect();
    assert!(
        unmarked.is_empty(),
        "the scan below only covers marked productions, and {} of them carry no note",
        unmarked.len()
    );

    // The claim: a word the grammar spells as a literal is either a reserved
    // keyword — §1.3's rule, that a word is listed only while the parser has a
    // rule that reads it — or it is named by the note that marks its production,
    // so a reader meets the word and the sentence that says it is not a keyword.
    let mut unnamed: Vec<String> = Vec::new();
    for (index, whole_block) in regions {
        let note = note_text(&lines, index);
        for at in marked_region(&lines, index, whole_block) {
            for word in literals(lines[at]) {
                if reserved.contains(&word) || contains_word(&note, &word) {
                    continue;
                }
                unnamed.push(format!("'{}' on line {}", word, at + 1));
            }
        }
    }

    assert!(
        unnamed.is_empty(),
        "docs/GRAMMAR.md spells {} word(s) as keywords in a production that carries no \
         Not-implemented note: {}. Either reserve the word in §1.3 and give the parser a rule \
         for it, or name it in the note above its production — a reader who meets `needs` in \
         the grammar has to meet the sentence that says it is not a keyword.",
        unnamed.len(),
        unnamed.join(", ")
    );

    // The three words this finding named. `needs` and `gives` are in neither
    // §1.3 nor a marked production's note as a *keyword*, and `default` is not
    // reserved either: they are ordinary identifiers, which is exactly what the
    // notes say. If a future phase gives the parser a rule for one, the notes
    // and this test have to change together.
    for word in ["needs", "gives", "default", "list", "record", "function"] {
        assert!(
            !Lexer::keywords().contains(&word),
            "`{}` is reserved by the lexer; if the parser now reads it, drop the Not-implemented \
             note it is listed under with the same change",
            word
        );
    }
}

/// The words a grammar line spells as a single-quoted literal: `'set'`, `'needs'`,
/// `'list of'` as `list` and `of`. Punctuation is left out — `'{'`, `':'` and
/// `','` are notation rather than words a writer types, and a one-letter literal
/// cannot be a keyword in any case.
fn literals(line: &str) -> Vec<String> {
    line.split('\'')
        .skip(1)
        .step_by(2)
        .flat_map(|literal| literal.split([' ', '/']))
        .filter(|word| word.len() > 1 && word.chars().all(|c| c.is_ascii_lowercase()))
        .map(|word| word.to_string())
        .collect()
}

/// Whether `text` names `word` as a whole word, which is what "the note names
/// this word" has to mean: a note about `of` is not satisfied by the word
/// `record of` in a production three lines away, and `needs` is not satisfied
/// by `needy`.
fn contains_word(text: &str, word: &str) -> bool {
    text.split(|c: char| !c.is_alphanumeric() && c != '_')
        .any(|piece| piece == word)
}

// ---------------------------------------------------------------------------
// 4. SPEC.md stops promising the forms
// ---------------------------------------------------------------------------

#[test]
fn every_spec_section_that_shows_type_syntax_says_it_is_not_implemented() {
    for heading in [
        "### Complex Types",
        "### Explicit Types",
        "### Type Annotations",
    ] {
        let section = section_containing(SPEC, heading);
        assert!(
            section.to_lowercase().contains("not implemented"),
            "SPEC.md `{}` shows type syntax the parser cannot read with no Not-implemented note",
            heading
        );
    }

    // The form itself may not appear in a section that does not say so, anywhere
    // in either document — the promise is what a reader copies out of it.
    for (name, document) in [("SPEC.md", SPEC), ("docs/GRAMMAR.md", GRAMMAR)] {
        for promise in [
            "set numbers to list of number",
            "record { x: number",
            "set x : number",
            "needs number",
            "a as number",
            "as number",
        ] {
            for section in sections_containing(document, promise) {
                assert!(
                    section.to_lowercase().contains("not implemented"),
                    "{} writes `{}` in `{}`, which does not say the form is Not implemented; \
                     the parser refuses it",
                    name,
                    promise,
                    heading_of(&section)
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 5. The bare forms still work
// ---------------------------------------------------------------------------

/// The declarations that carry an optional annotation still parse without it.
/// This is the half of the claim a "Not implemented" note can get wrong: mark
/// the annotation unimplemented and accidentally mark the declaration with it.
#[test]
fn edge_the_bare_form_of_every_annotated_declaration_still_runs() {
    for host in HOSTS {
        let tokens = Lexer::tokenize(host.bare)
            .unwrap_or_else(|e| panic!("{} without its annotation should lex: {}", host.name, e));
        let ast = redblue::parser::parse(tokens)
            .unwrap_or_else(|e| panic!("{} without its annotation should parse: {}", host.name, e));
        redblue::analyzer::analyze(&ast).unwrap_or_else(|e| {
            panic!("{} without its annotation should analyze: {}", host.name, e)
        });

        let (mut vm, result) = redblue::run_isolated(&ast);
        result.unwrap_or_else(|e| panic!("{} without its annotation should run: {}", host.name, e));
        let printed = vm.take_output();

        let expected: Vec<String> = host.printed.iter().map(|s| s.to_string()).collect();
        assert_eq!(
            printed, expected,
            "{} without its annotation printed the wrong thing",
            host.name
        );
    }
}

/// The annotation present, absent, malformed and doubled, on each of the five
/// hosts, as one readable table. The row above carries the same four states for
/// the bare forms; this one is the boundary matrix the finding asked for:
///
/// | host           | absent  | present        | malformed       | doubled            |
/// |----------------|---------|----------------|-----------------|--------------------|
/// | `set`          | runs    | refused        | refused         | refused            |
/// | `constant`     | runs    | refused        | refused         | refused            |
/// | `parameter`    | runs    | refused        | refused         | refused            |
/// | `has`          | runs    | refused        | refused         | refused            |
/// | `function`     | runs    | refused        | refused         | refused            |
#[test]
fn edge_annotation_absent_present_malformed_and_doubled_on_all_five_hosts() {
    for host in HOSTS {
        // Absent: parses. Checked here as well as in the test above so the four
        // states sit in one place and the matrix can be read against the code.
        assert!(
            refused(host.bare).is_none(),
            "{} without an annotation must parse, got: {:?}",
            host.name,
            refused(host.bare).map(|e| e.to_string())
        );

        for (state, source) in [
            ("present", host.annotated),
            ("malformed", host.malformed),
            ("doubled", host.doubled),
        ] {
            let error = expect_refusal(source, &format!("a {} annotation on {}", state, host.name));
            assert!(
                error.span().is_some(),
                "a {} annotation on {} must be refused with a position, got: {}",
                state,
                host.name,
                error
            );
            assert!(
                !error.to_string().is_empty(),
                "a {} annotation on {} must be refused with a message",
                state,
                host.name
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Section helpers
// ---------------------------------------------------------------------------

/// Every section of a Markdown document: the text from one heading to the next.
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

/// The section of `document` whose first heading line is `heading`.
fn section_containing(document: &str, heading: &str) -> String {
    sections_of(document)
        .into_iter()
        .find(|section| section.lines().next().map(|l| l.trim()) == Some(heading))
        .unwrap_or_else(|| panic!("the document must keep a `{}` section", heading))
}

/// The whole chapter of a Markdown document that starts at `heading`: the
/// heading line to the next heading at the same level or above it. A reader who
/// searches for "type" lands on `## 2. Type System`, not on `### 2.1`.
fn chapter_containing(document: &str, heading: &str) -> String {
    let level = heading.chars().take_while(|c| *c == '#').count();
    let body = document
        .split(heading)
        .nth(1)
        .unwrap_or_else(|| panic!("the document must keep a `{}` section", heading));

    let mut out = String::from(heading);
    let mut fence = false;
    for line in body.lines() {
        let closes = line.trim_start().starts_with("```");
        if line.starts_with('#')
            && !fence
            && line.chars().take_while(|c| *c == '#').count() <= level
        {
            break;
        }
        if closes {
            fence = !fence;
        }
        out.push('\n');
        out.push_str(line);
    }
    out
}

/// The heading line of a section, for a message that has to say where it looked.
fn heading_of(section: &str) -> &str {
    section
        .lines()
        .find(|line| line.starts_with('#'))
        .unwrap_or("<no heading>")
}
