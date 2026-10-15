//! Live syntax highlighting for the line editor, taken from Redblue's own
//! lexer.
//!
//! The whole point is that there is no second classifier. Ipp ships a
//! hand-written lexer of its own for its prompt, because a Python host cannot
//! borrow the one it already has; Redblue *is* the language, so the editor asks
//! [`crate::lexer::Lexer`] the same question the parser asks and takes the
//! answer it gets. A word that stops being a keyword stops being coloured the
//! moment the lexer stops reserving it — there is no second table to forget to
//! update.
//!
//! Two rules govern what comes out, and both exist because this runs on a line
//! the user is still in the middle of typing:
//!
//! 1. **A line the lexer rejects comes back plain.** Half a line is being typed
//!    all the time, and the usual half-written line is one the lexer cannot read
//!    yet. Colouring what it can read would leave the text the user is looking
//!    at disagreeing with the text being run, and an unterminated string is the
//!    most common line there is. This is display-only: nothing here can change
//!    what gets executed, because nothing here is on the path to the parser.
//! 2. **Colour is off means no escapes at all.** A session with colour off gets
//!    the line back as it was typed, which is also what a piped session gets,
//!    because a piped session is never given an editor to colour.

use crate::lexer::{Lexer, Token, TokenKind};
use crate::stdlib::BUILTIN_NAMES;

use super::style::{Style, Theme, TokenClass};

/// The line as [`Lexer`] should read it, to make comments visible.
///
/// The lexer *skips* comments: they are not tokens, so a plain tokenize of
/// `say 1 // count` cannot say where the comment began, only that something
/// between the `1` and the end of the line was not a token. Recovering that
/// from token positions alone is impossible — the token before a trailing
/// comment has no end position, only the start of whatever came after, which is
/// the end of the line.
///
/// So the line is rewritten before it is read: every `//` becomes `!!`, two
/// characters for two, so every position the lexer reports is the position it
/// would have reported for the line as written. `!` is an operator the lexer
/// already has, so `!!` lexes to two `Not` tokens where the comment used to be
/// invisible, and a `//` *inside* a string stays inside the string token because
/// the substitution does not change which quote opens or closes it.
///
/// The cost is one pass over the line, not two: the rewritten line is the only
/// one lexed.
fn mark_comments(line: &str) -> String {
    let mut marked = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '/' && chars.peek() == Some(&'/') {
            chars.next();
            marked.push_str("!!");
        } else {
            marked.push(ch);
        }
    }
    marked
}

/// One byte range of the line as typed, and what the lexer found there.
struct Run {
    start: usize,
    end: usize,
    class: TokenClass,
}

/// The line as it should be drawn.
///
/// Returns `line` unchanged when colour is off and when the lexer rejects the
/// line — in both cases byte for byte what was passed in, because the caller
/// is about to hand the *typed* line to the parser and a highlighter that
/// disagreed with it by a byte would be a bug with no visible cause.
pub fn highlight(line: &str, style: &Style, theme: &Theme) -> String {
    if !style.enabled() {
        return line.to_string();
    }
    match runs(line) {
        Some(runs) => render(line, style, theme, &runs),
        None => line.to_string(),
    }
}

/// The non-plain byte ranges of `line`, in the order they appear, or `None`
/// when the line is not one this can read.
fn runs(line: &str) -> Option<Vec<Run>> {
    // A line editor asks about one line at a time. A caller handing over more
    // than that gets the whole thing plain, which is not a wrong answer so much
    // as no answer at all — and better than guessing where lines begin.
    if line.contains('\n') {
        return None;
    }
    // The rewritten line, which is the only one lexed, has the same number of
    // characters as the line as written, so every column it reports is a valid
    // index into this one's characters.
    let offsets = byte_offsets(line);
    // One pass, because the walk back over trailing whitespace below asks this
    // once per token, and asking the line itself would make a long line cost a
    // scan of itself per token — quadratic on exactly the input a benchmark
    // test exists to catch.
    let whitespace: Vec<bool> = line.chars().map(char::is_whitespace).collect();
    let chars = whitespace.len();
    let tokens: Vec<Token> = Lexer::tokenize(&mark_comments(line)).ok()?;

    // The start of each token, as an index into `line`'s characters, and the
    // token itself. `Eof` and `Newline` are not on the line the user can see,
    // so they carry no position worth painting.
    let mut starts: Vec<(usize, &Token)> = Vec::with_capacity(tokens.len());
    for token in &tokens {
        if matches!(token.kind, TokenKind::Eof | TokenKind::Newline) {
            continue;
        }
        starts.push((token.column.saturating_sub(1), token));
    }

    // A comment runs to the end of the line, so it is always the last thing on
    // it: nothing the lexer read can follow one. Two `Not` tokens on adjacent
    // columns are the `!!` a `//` became, and the first such pair is where the
    // comment began.
    let comment_at = starts
        .windows(2)
        .find(|pair| is_not(pair[0].1) && is_not(pair[1].1) && pair[1].0 == pair[0].0 + 1)
        .map(|pair| pair[0].0);
    if let Some(at) = comment_at {
        starts.retain(|(start, _)| *start < at);
    }

    let mut runs = Vec::new();
    for (index, (start, token)) in starts.iter().enumerate() {
        let Some(class) = class_of(token) else {
            continue;
        };
        // A token ends where the next one starts, and nowhere else: it is the
        // only end position the lexer can give. Trailing whitespace between the
        // two belongs to neither, so it is walked back off.
        let limit = comment_at.unwrap_or(chars);
        let mut end = starts.get(index + 1).map_or(limit, |(next, _)| *next);
        end = end.min(limit);
        while end > *start && whitespace[end - 1] {
            end -= 1;
        }
        if end <= *start {
            continue;
        }
        runs.push(Run {
            start: *offsets.get(*start)?,
            end: *offsets.get(end)?,
            class,
        });
    }

    if let Some(at) = comment_at {
        let start = *offsets.get(at)?;
        if start < line.len() {
            runs.push(Run {
                start,
                end: line.len(),
                class: TokenClass::Comment,
            });
        }
    }
    Some(runs)
}

/// The class the lexer put `token` in, or `None` when the theme has nothing to
/// say about it and the text is better left plain.
///
/// `token.word` is what settles the keywords rather than a hand-written list:
/// it is set by exactly the tokens the lexer read from [`crate::lexer::KEYWORDS`],
/// and it distinguishes the words that share a kind from the symbols that make
/// it — `mod` is a keyword, `%` is the same token and is an operator, and
/// neither needed a line here to say so.
fn class_of(token: &Token) -> Option<TokenClass> {
    if token.word.is_some() {
        return Some(TokenClass::Keyword);
    }
    match &token.kind {
        TokenKind::Text(_) => Some(TokenClass::Str),
        TokenKind::Number(_) => Some(TokenClass::Number),
        TokenKind::Identifier(name) => is_builtin(name).then_some(TokenClass::Builtin),
        TokenKind::Plus
        | TokenKind::Minus
        | TokenKind::Star
        | TokenKind::Slash
        | TokenKind::Mod
        | TokenKind::Not
        | TokenKind::Equal
        | TokenKind::NotEqual
        | TokenKind::Less
        | TokenKind::LessEqual
        | TokenKind::Greater
        | TokenKind::GreaterEqual => Some(TokenClass::Operator),
        _ => None,
    }
}

/// The classes a theme needs, drawn from the standard library's own name table
/// so a builtin cannot be one colour today and another tomorrow.
fn is_builtin(name: &str) -> bool {
    BUILTIN_NAMES.contains(&name)
}

fn is_not(token: &Token) -> bool {
    matches!(token.kind, TokenKind::Not)
}

/// The byte offset of every character in `line`, plus the end of it, so a
/// column the lexer reported can be turned into a slice boundary without
/// walking the line once per token.
fn byte_offsets(line: &str) -> Vec<usize> {
    let mut offsets: Vec<usize> = line.char_indices().map(|(offset, _)| offset).collect();
    offsets.push(line.len());
    offsets
}

/// `line` with each run painted, and nothing else changed.
///
/// A run whose class the theme has no colour for is copied across unaltered,
/// which is why a theme is allowed to be incomplete.
fn render(line: &str, style: &Style, theme: &Theme, runs: &[Run]) -> String {
    let mut out = String::with_capacity(line.len());
    let mut at = 0;
    for run in runs {
        // Overlapping or out-of-order runs would be a bug in `runs`, not
        // something to draw: skipping one leaves the text readable.
        if run.start < at || run.end > line.len() || run.end <= run.start {
            continue;
        }
        out.push_str(&line[at..run.start]);
        let text = &line[run.start..run.end];
        match theme.role(run.class) {
            Some(role) => out.push_str(&style.paint(role, text)),
            None => out.push_str(text),
        }
        at = run.end;
    }
    out.push_str(&line[at.min(line.len())..]);
    out
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{highlight, mark_comments};
    use crate::lexer::Lexer;
    use crate::repl::style::{Paint, Style, Theme, TokenClass};

    /// Colour on, which is the only way an escape-code assertion runs at all:
    /// tests cannot make stdout a terminal.
    fn on() -> Style {
        Style::forced(true)
    }

    /// `painted` — the escape sequence a role is wrapped in — with its text.
    fn painted(role: Paint, text: &str) -> String {
        on().paint(role, text)
    }

    /// What the line looks like with every escape sequence removed, which is
    /// the property the whole module exists to keep.
    fn strip(coloured: &str) -> String {
        let mut plain = String::new();
        let mut chars = coloured.chars();
        while let Some(ch) = chars.next() {
            if ch != '\u{1b}' {
                plain.push(ch);
                continue;
            }
            // Skip `ESC [ … m`, which is every sequence this emits.
            for skipped in chars.by_ref() {
                if skipped == 'm' {
                    break;
                }
            }
        }
        plain
    }

    /// The escape sequence a role opens a run with, which every coloured run
    /// emits exactly once.
    fn prefix(role: Paint) -> String {
        let wrapped = on().paint(role, "");
        wrapped
            .strip_suffix("\u{1b}[0m")
            .unwrap_or(&wrapped)
            .to_string()
    }

    /// How many runs on `line` take `role`.
    fn count_painted(line: &str, role: Paint) -> usize {
        highlight(line, &on(), &Theme::standard())
            .matches(&prefix(role))
            .count()
    }

    #[test]
    fn a_keyword_and_a_string_each_take_their_own_colour() {
        let line = "say \"hi\"";
        let coloured = highlight(line, &on(), &Theme::standard());

        assert_eq!(
            strip(&coloured),
            line,
            "the drawn line reads exactly as it was typed once the colour is taken off"
        );
        assert!(
            coloured.contains(&painted(Paint::Keyword, "say")),
            "`say` is a reserved word and takes the keyword colour, got {}",
            coloured.escape_debug()
        );
        assert!(
            coloured.contains(&painted(Paint::Str, "\"hi\"")),
            "a string literal takes the string colour, quotes and all, got {}",
            coloured.escape_debug()
        );
        assert_eq!(
            count_painted(line, Paint::Keyword),
            1,
            "only `say` is a keyword on this line"
        );
    }

    #[test]
    fn a_line_the_lexer_rejects_comes_back_plain_and_byte_identical() {
        // An unterminated string is the commonest half-typed line there is,
        // and it is the one the editor most wants to colour.
        let line = "say \"hi";
        assert!(
            Lexer::tokenize(line).is_err(),
            "the line under test must be one the lexer actually rejects, or this proves nothing"
        );

        let coloured = highlight(line, &on(), &Theme::standard());
        assert_eq!(
            coloured, line,
            "a line the lexer rejects is drawn plain, so nothing can colour a program that runs"
        );
        assert!(
            !coloured.contains('\u{1b}'),
            "and plain means no escape sequences at all, got {}",
            coloured.escape_debug()
        );
    }

    #[test]
    fn an_unexpected_character_is_also_a_failure_rather_than_a_colour() {
        // The other shape of rejection: no string to be unterminated, just a
        // character the language has no token for.
        let line = "say \u{1}";
        assert!(
            Lexer::tokenize(line).is_err(),
            "a control character has no token, so the lexer must refuse it"
        );
        assert_eq!(
            highlight(line, &on(), &Theme::standard()),
            line,
            "and an unexpected character leaves the line plain"
        );
    }

    #[test]
    fn numbers_comments_and_builtins_are_told_apart() {
        let theme = Theme::standard();
        let on = on();

        assert!(
            highlight("set total to 42", &on, &theme).contains(&painted(Paint::Number, "42")),
            "a numeric literal takes the number colour"
        );
        assert!(
            highlight("set total to 42", &on, &theme).contains(&painted(Paint::Keyword, "set")),
            "and `set` is still a keyword on the same line"
        );
        assert!(
            highlight("set x to uppercase(\"a\")", &on, &theme)
                .contains(&painted(Paint::Builtin, "uppercase")),
            "a standard-library name takes the builtin colour"
        );
        assert!(
            highlight("set x to total + 1", &on, &theme).contains(&painted(Paint::Operator, "+")),
            "an operator takes the operator colour"
        );

        // `//` is not a token — the lexer skips comments — so this colour comes
        // from somewhere other than the token stream.
        let commented = highlight("set x to 1 // the answer", &on, &theme);
        assert!(
            commented.contains(&painted(Paint::Comment, "// the answer")),
            "a comment to the end of the line takes the comment colour, got {}",
            commented.escape_debug()
        );
        assert_eq!(
            strip(&commented),
            "set x to 1 // the answer",
            "and the comment is still every character the user typed"
        );
    }

    #[test]
    fn a_slash_inside_a_string_is_text_and_not_a_comment() {
        let line = "set url to \"http://example.com\"";
        let coloured = highlight(line, &on(), &Theme::standard());

        assert!(
            !coloured.contains(&painted(Paint::Comment, "//")),
            "a `//` inside a string literal is part of the string, not a comment"
        );
        assert!(
            coloured.contains(&painted(Paint::Str, "\"http://example.com\"")),
            "and the whole literal stays one string-coloured run, got {}",
            coloured.escape_debug()
        );
    }

    #[test]
    fn colour_off_returns_the_line_untouched() {
        let line = "say \"hi\"";
        let off = Style::forced(false);

        assert_eq!(
            highlight(line, &off, &Theme::standard()),
            line,
            "a session with colour off gets the line back exactly, not a stripped copy of it"
        );
        assert!(
            !highlight(line, &off, &Theme::standard()).contains('\u{1b}'),
            "and emits no escape sequence even though the theme would colour every token"
        );
    }

    #[test]
    fn a_multibyte_string_is_one_run_and_never_split() {
        // Each of these is more than one byte and more than one char, and a
        // colour run that started or ended mid-character would panic on the
        // slice or emit a broken escape sequence.
        for text in [
            "\"héllo\"",
            "\"日本語\"",
            "\"\u{1F600}\u{1F680}\"",
            "\"e\u{301}\u{332}\"",
        ] {
            let line = format!("say {text}");
            let coloured = highlight(&line, &on(), &Theme::standard());

            assert_eq!(
                strip(&coloured),
                line,
                "{text:?} reads back exactly once the colour is off"
            );
            assert!(
                coloured.contains(&painted(Paint::Str, text)),
                "{text:?} is coloured as one whole run rather than split, got {}",
                coloured.escape_debug()
            );
            // Two runs on every one of these lines — `say` and the string — and
            // each run emits an opening and a closing escape, so four escapes
            // in total. More would mean a run was split, which is the bug.
            assert_eq!(
                coloured.matches('\u{1b}').count(),
                4,
                "{text:?} is drawn as two whole runs and nothing else, got {}",
                coloured.escape_debug()
            );
        }
    }

    #[test]
    fn edge_an_empty_line_is_plain_and_costs_nothing() {
        let line = "";
        assert_eq!(
            highlight(line, &on(), &Theme::standard()),
            "",
            "there is nothing on an empty line to colour"
        );
        assert!(
            !highlight(line, &on(), &Theme::standard()).contains('\u{1b}'),
            "and an empty line is drawn without a stray escape sequence"
        );
    }

    #[test]
    fn edge_a_line_that_is_only_whitespace_is_left_where_it_is() {
        for line in ["   ", "\t", "\u{00A0}\u{3000}"] {
            assert_eq!(
                highlight(line, &on(), &Theme::standard()),
                line,
                "a line of nothing but spaces is drawn exactly as typed"
            );
        }
    }

    #[test]
    fn edge_a_theme_missing_a_class_leaves_its_tokens_plain() {
        // A theme is the part a user replaces, so a hole in one is ordinary.
        // `TokenClass::Str` is the hole here; everything else still paints.
        let theme = Theme::standard().without(TokenClass::Str);
        let line = "set greeting to \"hi\"";

        let coloured = highlight(line, &on(), &theme);
        assert_eq!(
            strip(&coloured),
            line,
            "a theme with a hole still draws every character it was given"
        );
        assert!(
            !coloured.contains(&painted(Paint::Str, "\"hi\"")),
            "the class the theme has nothing to say about is left plain"
        );
        assert!(
            coloured.contains(&painted(Paint::Keyword, "set")),
            "and the classes it does have an opinion about are unaffected"
        );
        assert_eq!(
            theme.role(TokenClass::Str),
            None,
            "which is exactly what `role` reports for the class that was removed"
        );
        assert_eq!(
            Theme::standard().role(TokenClass::Comment),
            Some(Paint::Comment),
            "and the shipped theme has an opinion about every class it lists"
        );
    }

    #[test]
    fn edge_a_ten_kilobyte_line_is_highlighted_without_blowing_up() {
        let style = on();
        let theme = Theme::standard();

        // Two shapes on one line, because they fail in different ways. One very
        // long identifier is a single token the lexer walks once; several
        // thousand small ones are several thousand tokens, each one walking back
        // over the whitespace behind it — which is the loop that turns linear
        // work into quadratic the moment anything on it scans the line.
        let one_token = format!("say {}", "x".repeat(10_000));
        let many_tokens: String = (0..1_000)
            .map(|n| format!("set v{n} to {n} "))
            .collect::<String>()
            .trim_end()
            .to_string();

        for line in [&one_token, &many_tokens] {
            let started = Instant::now();
            let coloured = highlight(line, &style, &theme);
            let spent = started.elapsed();

            assert_eq!(strip(&coloured), *line, "the line reads back exactly");
            assert!(
                spent < Duration::from_millis(500),
                "a {} byte line took {spent:?}, which is quadratic behaviour rather than fast",
                line.len()
            );
        }
        assert!(
            many_tokens.len() > 10_000,
            "the many-token line is meant to be the larger of the two, got {} bytes",
            many_tokens.len()
        );
    }

    #[test]
    fn two_thousand_lines_of_real_code_highlight_in_under_a_second() {
        let source = include_str!("../../bootstrap/compiler.rb");
        let lines: Vec<&str> = source.lines().collect();
        assert!(
            !lines.is_empty(),
            "the bootstrap compiler is what this benchmarks, and it is not empty"
        );

        let mut corpus: Vec<&str> = Vec::with_capacity(2000);
        while corpus.len() < 2000 {
            corpus.extend(lines.iter().copied());
        }
        corpus.truncate(2000);

        let started = Instant::now();
        let mut painted = 0usize;
        for line in &corpus {
            painted += highlight(line, &on(), &Theme::standard()).len();
        }
        let spent = started.elapsed();

        assert!(
            painted > 0,
            "the corpus produced no output at all, so the timing below means nothing"
        );
        assert!(
            spent < Duration::from_secs(1),
            "2000 lines of the bootstrap compiler took {spent:?}, which is over the budget"
        );
    }

    #[test]
    fn the_marked_line_is_the_same_length_so_every_column_still_lands() {
        // The whole trick rests on this: the rewritten line has to be exactly
        // as many characters as the line as written, or every position the
        // lexer reports would point somewhere else in it.
        for line in [
            "say \"hi\"",
            "set url to \"http://example.com\" // where",
            "//",
            "///",
            "/",
            "say 1 / 2",
            "say !!",
            "// // //",
        ] {
            let marked = mark_comments(line);
            assert_eq!(
                marked.chars().count(),
                line.chars().count(),
                "{line:?} must survive the rewrite at the same length"
            );
            assert_eq!(
                marked.len(),
                line.len(),
                "{line:?} must survive the rewrite at the same byte length"
            );
        }
        assert_eq!(
            mark_comments("set url to \"http://example.com\" // where"),
            "set url to \"http:!!example.com\" !! where",
            "a `//` is rewritten wherever it stands, including inside a string"
        );
    }

    #[test]
    fn edge_a_bare_bang_is_an_operator_and_a_doubled_one_is_a_comment() {
        // `!` is the operator and `!!` is what a `//` is rewritten to, so this
        // is where the rewrite could be confused for something the user typed.
        let theme = Theme::standard();
        let on = on();

        assert!(
            highlight("say not true", &on, &theme).contains(&painted(Paint::Keyword, "not")),
            "the word `not` is a keyword"
        );
        assert!(
            !highlight("say not true", &on, &theme).contains(&painted(Paint::Comment, "not")),
            "and is not mistaken for the start of a comment"
        );
        assert!(
            highlight("say !!", &on, &theme).contains(&painted(Paint::Comment, "!!")),
            "two bangs in a row are a comment, exactly as two slashes are"
        );
        assert!(
            highlight("set ok to yes", &on, &theme).contains(&painted(Paint::Keyword, "yes")),
            "`yes` is a keyword token and is coloured as one"
        );
        assert!(
            highlight("set r to 5 mod 2", &on, &theme).contains(&painted(Paint::Keyword, "mod")),
            "and `mod` is a keyword even though `%` is the same token"
        );
        assert!(
            highlight("set r to 5 % 2", &on, &theme).contains(&painted(Paint::Operator, "%")),
            "while the symbol that makes the same token is an operator"
        );
    }
}
