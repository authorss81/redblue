use super::commands;

/// The module names a program can write as a receiver of a dotted call, read
/// from the one table the analyzer reads them from.
///
/// A module is the only vocabulary the completer offers that the language
/// *decides*: `json.parse` is a name or it is `Unknown variable 'json'`, and the
/// REPL should not be the place a user finds out. [`crate::stdlib::MODULES`] is
/// that decision, and [`crate::analyzer`] asks it, so a name added there is
/// completed here without a second list to remember to change.
fn module_names() -> Vec<String> {
    crate::stdlib::MODULES
        .iter()
        .map(|name| (*name).to_string())
        .collect()
}

/// The words a program calls without a receiver, read from the registry rather
/// than written out again.
///
/// Every name is one a program can call: a module, a flat builtin
/// ([`crate::stdlib::flat_builtin_names`]) or one of the two
/// [`crate::stdlib::GLOBAL_CONSTANTS`]. A flat builtin *encoding* is the same
/// mistake this table used to make in its other direction: `read` and `write`
/// are real functions that live under `files.` and answer `Unknown function` on
/// their own.
///
/// **And every one of them answers.** That half is not decoration: `filter` and
/// `reduce` are registered, so `is_builtin` says true and the table that used to
/// be the parity gate asked no further question — while a user who completed
/// `filt` was handed a name whose every call is `filter was called with arguments
/// it cannot use`. [`crate::stdlib::flat_builtin_names`] drops the names nothing
/// implements, so the offer and the call are the same claim; `tests/
/// repl_module_completion_test.rs` walks the whole offered list and runs it.
fn builtin_names() -> Vec<String> {
    module_names()
        .into_iter()
        .chain(
            crate::stdlib::flat_builtin_names()
                .into_iter()
                .map(|name| name.to_string()),
        )
        .chain(
            crate::stdlib::GLOBAL_CONSTANTS
                .iter()
                .map(|name| (*name).to_string()),
        )
        .collect()
}

/// The words the completer offers that [`crate::lexer::KEYWORDS`] does not hold.
///
/// There is one, and it is here rather than in the hand-kept table because it is
/// a decision about the language rather than a copy of it. `by` is not a reserved
/// word — `Parser::at_range_step_marker` (`src/parser.rs:954`) reads it
/// positionally, which is what keeps `to can grow(by)` and `say by` legal programs
/// — but it is still a word the language *starts a clause with*: `for each i from
/// 0 to 10 by 5` is a stepped loop and `split("a,b", by ",")` labels an argument.
/// A user typing `by` at either place is offered it for that reason.
///
/// It is the only such word, and the gate
/// [`edge_the_keyword_vocabulary_is_the_lexers_own_table`] fails if a second one
/// turns up without a rule of its own.
const POSITIONAL_KEYWORDS: &[&str] = &["by"];

/// The language's keywords, read from the table the lexer lexes from.
///
/// A keyword is the one piece of vocabulary the *lexer* decides, so
/// [`crate::lexer::KEYWORDS`] is the table to read — the same reason the command
/// list is read from [`commands::ReplCommand`] and the module list from
/// [`crate::stdlib::MODULES`]. The hand-kept copy had drifted in both directions:
/// it offered `are`, `ask`, `new`, `async`, `done`, `parallel`, `wait` and `when`,
/// and not one of those eight is a keyword the lexer reserves or a word any
/// program can contain — `say new` is `Unknown variable 'new'` — so a user who
/// typed `ne` and pressed Tab was handed a word that cannot be written; and it did
/// not offer `constant`, `as`, `test`, `expect`, `might` or `fail`, all of which
/// begin statements. So a user who typed `as` was offered `ask`.
///
/// **A phrase, not a word, where the language spells it as one.** The lexer gives
/// the two words of `give back` and of `might fail` the *same* token, and the
/// language writes both as the pair. Words sharing a kind are offered joined, and
/// which words those are is [`crate::lexer::KEYWORD_PHRASES`] — the lexer's own
/// declaration, not the contiguity of [`crate::lexer::KEYWORDS`], which nothing
/// promises and which alphabetising that table would have silently unjoined.
///
/// The tail of a pair is not offered as a word of its own, and that follows from
/// the joining rather than from a rule beside it: completion is a prefix match,
/// so `back` on its own would only ever match after a user had already typed
/// `give back` — and complete it to a second `back`. The tail is still a
/// *vocabulary member*, through the phrase that carries it: `give back` is in
/// the vocabulary and `give` completes to it.
fn keyword_names() -> Vec<String> {
    let phrases = crate::lexer::KEYWORD_PHRASES;

    crate::lexer::KEYWORDS
        .iter()
        .filter_map(|(word, _)| {
            match phrases.iter().find(|(head, _)| head == word) {
                Some((_, tail)) => Some(format!("{word} {tail}")),
                // The tail is offered with its head, in the order the phrase is
                // written rather than the order `KEYWORDS` happens to list it in.
                None if phrases.iter().any(|(_, tail)| tail == word) => None,
                None => Some((*word).to_string()),
            }
        })
        .chain(POSITIONAL_KEYWORDS.iter().map(|word| (*word).to_string()))
        .collect()
}

pub struct ReplCompleter {
    keywords: Vec<String>,
    builtins: Vec<String>,
    commands: Vec<String>,
}

impl ReplCompleter {
    pub fn new() -> Self {
        Self {
            keywords: keyword_names(),
            builtins: builtin_names(),
            // The REPL's own command words, read from the table the parser
            // accepts them in rather than written out again. A hand-kept list
            // here is the second table: it held the sixteen canonical names
            // while the parser took thirty aliases, so `:q`, `:h`, `:hist`,
            // `:v`, `:f`, `:l`, `:s`, `:i` and `:examples` all worked and none
            // of them was completable.
            //
            // The words, not a spelling of them: which sigil the user types is
            // theirs — see [`commands::SIGILS`] — and `complete_with` puts back
            // whichever of them they began with.
            commands: commands::ReplCommand::bare_names(),
        }
    }

    pub fn complete(&self, word: &str) -> Vec<String> {
        self.complete_with(word, &[])
    }

    /// Completes `word` against the language's own vocabulary plus `names`, the
    /// bindings a live session has made — its variables, the functions it
    /// defined and the modules it imported.
    ///
    /// The static tables alone can never offer a session's own names, so a
    /// completer built from them would finish `cou` to nothing in a session
    /// holding `count`.
    ///
    /// A word that begins with one of [`commands::SIGILS`] is completed against
    /// the commands alone and offered back with that sigil: `:q` and `.q` both
    /// reach the same table, and a user who typed the second one was offered the
    /// second one. With no sigil — or an empty word — the canonical `:` spelling
    /// is what every vocabulary is offered in.
    pub fn complete_with(&self, word: &str, names: &[String]) -> Vec<String> {
        let mut matches = Vec::new();
        let sigil = word.chars().next().filter(|c| commands::SIGILS.contains(c));
        let (command_sigil, word_lower) = match sigil {
            Some(sigil) => (sigil, word[sigil.len_utf8()..].to_lowercase()),
            None => (':', word.to_lowercase()),
        };

        for keyword in &self.keywords {
            if sigil.is_none() && keyword.to_lowercase().starts_with(&word_lower) {
                matches.push(keyword.clone());
            }
        }

        for builtin in &self.builtins {
            if sigil.is_none() && builtin.to_lowercase().starts_with(&word_lower) {
                matches.push(builtin.clone());
            }
        }

        // An empty word asks what could go here, which includes the commands;
        // any other word only reaches them through a sigil, or `hi` would offer
        // `:hist`.
        if sigil.is_some() || word.is_empty() {
            for command in &self.commands {
                if command.to_lowercase().starts_with(&word_lower) {
                    matches.push(format!("{command_sigil}{command}"));
                }
            }
        }

        for name in names {
            if sigil.is_none() && name.to_lowercase().starts_with(&word_lower) {
                matches.push(name.clone());
            }
        }

        matches.sort();
        matches.dedup();

        matches
    }
}

impl Default for ReplCompleter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{ReplCompleter, POSITIONAL_KEYWORDS};
    use crate::repl::commands::{self, ReplCommand};

    /// The names a session would offer: a variable it bound, a function it
    /// defined and a module it imported.
    fn session_names() -> Vec<String> {
        vec![
            "counter".to_string(),
            "circle_area".to_string(),
            "MathUtils".to_string(),
        ]
    }

    /// The kind [`crate::lexer::KEYWORDS`] gives `word`, or a failure naming the
    /// word — so a gate below reads the lexer's own table rather than a copy of
    /// it, which is the whole point of reading it.
    fn lexer_kind(word: &str) -> &'static crate::lexer::TokenKind {
        crate::lexer::KEYWORDS
            .iter()
            .find(|(listed, _)| *listed == word)
            .map(|(_, kind)| kind)
            .unwrap_or_else(|| panic!("{word} is not a keyword the lexer reserves"))
    }

    /// Runs `source` through the whole pipeline the CLI runs — lexer, parser,
    /// analyzer, then the tree-walking VM — and answers what it printed, or the
    /// message it was refused with.
    ///
    /// One helper rather than two, because a gate that asks "is this spelling
    /// still *reached*" needs the answer to be allowed to be a run, and a gate
    /// that asks "is this spelling refused" needs it not to be. [`printed`] and
    /// [`refuses`] are the two questions, and each says what it means when the
    /// answer is the other one.
    #[track_caller]
    fn run(source: &str) -> Result<Vec<String>, String> {
        let tokens = crate::lexer::Lexer::tokenize(source)
            .unwrap_or_else(|error| panic!("`{source}` should lex, got {error:?}"));
        let program = crate::parser::parse(tokens)
            .unwrap_or_else(|error| panic!("`{source}` should parse, got {error:?}"));
        crate::analyzer::analyze(&program)
            .unwrap_or_else(|error| panic!("`{source}` should analyze, got {error:?}"));
        let mut vm = crate::Vm::new();
        match vm.run(&program) {
            Ok(_) => Ok(vm.take_output()),
            Err(error) => Err(format!("{error:?}")),
        }
    }

    /// The lines `source` prints, through the whole pipeline the CLI runs.
    #[track_caller]
    fn printed(source: &str) -> Vec<String> {
        run(source).unwrap_or_else(|refusal| panic!("`{source}` was refused: {refusal}"))
    }

    /// The message `source` is refused with — a helper rather than an `Option` so
    /// the gate that reads it fails with the message the language gave rather
    /// than with `None`.
    #[track_caller]
    fn refuses(source: &str) -> String {
        match run(source) {
            Ok(_) => panic!("`{source}` was accepted, so it is a name a program can call"),
            Err(error) => error,
        }
    }

    #[test]
    fn complete_matches_a_variable_the_session_bound() {
        let matches = ReplCompleter::new().complete_with("count", &session_names());

        assert_eq!(matches, vec!["counter".to_string()]);
    }

    #[test]
    fn complete_matches_a_function_the_session_defined() {
        let matches = ReplCompleter::new().complete_with("circle", &session_names());

        assert_eq!(matches, vec!["circle_area".to_string()]);
    }

    #[test]
    fn complete_matches_a_module_the_session_imported() {
        let matches = ReplCompleter::new().complete_with("math", &session_names());

        assert!(
            matches.contains(&"MathUtils".to_string()),
            "a module name is not a keyword and the static tables cannot know it: {:?}",
            matches
        );
    }

    #[test]
    fn complete_matches_a_builtin() {
        let matches = ReplCompleter::new().complete_with("upper", &[]);

        assert_eq!(matches, vec!["uppercase".to_string()]);
    }

    #[test]
    fn complete_matches_a_keyword() {
        assert_eq!(
            ReplCompleter::new().complete("whil"),
            vec!["while".to_string()]
        );
        assert_eq!(
            ReplCompleter::new().complete("giv"),
            vec!["give back".to_string()],
            "a multi-word keyword is offered whole"
        );
        assert_eq!(
            ReplCompleter::new().complete("migh"),
            vec!["might fail".to_string()],
            "`might fail` is the other pair the lexer gives one kind, so it is \
             offered whole as well rather than as two half-guards"
        );
        assert_eq!(
            ReplCompleter::new().complete("back"),
            Vec::<String>::new(),
            "and the tail of a pair is not a word on its own: a user who has typed \
             `give back` would be given a second `back`"
        );
    }

    /// Both words of `give back` and of `might fail` are keywords, so a gate that
    /// walked `lexer::KEYWORDS` word by word would ask for `back` and `fail` to be
    /// offered as words of their own — and the language writes each pair as one
    /// keyword. This pins the halves apart before the parity gate below counts on
    /// it: the head of each pair completes to the phrase, and the tail completes to
    /// nothing, because completion is a prefix match and `back` would otherwise be
    /// offered to a user who had already typed `give back`.
    #[test]
    fn edge_a_half_of_a_two_word_keyword_is_not_offered_on_its_own() {
        let completer = ReplCompleter::new();

        for (word, phrase) in [("give", "give back"), ("might", "might fail")] {
            let offered = completer.complete(word);
            assert!(
                offered.contains(&phrase.to_string()),
                "{word} is half of a phrase, so typing it offers the phrase: {offered:?}"
            );
        }
        for word in ["back", "fail"] {
            assert!(
                completer.complete(word).is_empty(),
                "{word} is the tail of a keyword the language spells as a pair, so it \
                 is not a word to complete on its own: {:?}",
                completer.complete(word)
            );
        }
    }

    /// The keyword vocabulary is the lexer's own table, in both directions.
    ///
    /// This is the same gate as
    /// [`edge_the_command_vocabulary_is_the_parsers_own_table`] one table over: a
    /// hand-kept keyword list beside `lexer::KEYWORDS` is a second answer to a
    /// question that already has one, and it had drifted both ways — it offered
    /// `new` and `ask`, which start nothing (`say new` is
    /// `Unknown variable 'new'`), so a user typing `ne` or `as` was handed a word
    /// no program can contain, and it did not offer `constant`, `as`, `test`,
    /// `expect`, `might` or `fail`, which do start statements.
    ///
    /// The two directions are the two halves of that: every keyword the lexer
    /// reserves is offered, and everything offered is one the lexer reserves or
    /// the single word in [`POSITIONAL_KEYWORDS`] that it does not.
    #[test]
    fn edge_the_keyword_vocabulary_is_the_lexers_own_table() {
        let completer = ReplCompleter::new();

        for (word, _) in crate::lexer::KEYWORDS {
            assert!(
                completer
                    .keywords
                    .iter()
                    .any(|entry| entry.split(' ').any(|part| part == *word)),
                "{word} is a keyword the lexer reserves, so it is offered: {:?}",
                completer.keywords
            );
        }

        for entry in &completer.keywords {
            if POSITIONAL_KEYWORDS.contains(&entry.as_str()) {
                continue;
            }
            let words: Vec<&str> = entry.split(' ').collect();
            let kind = lexer_kind(words[0]);
            for word in &words {
                assert_eq!(
                    lexer_kind(word),
                    kind,
                    "{entry} is offered as a keyword, so {word} is one the lexer \
                     reserves — and a phrase of two words is only a phrase if both \
                     halves lex to the same kind"
                );
            }
        }
    }

    /// Every multi-word keyword is offered whole when its first word is typed, and
    /// `might fail` is here to show the join is derived rather than special-cased:
    /// a table that joined `give`+`back` and offered `might` and `fail` separately
    /// would answer `migh` with `might` and leave the guard half-written.
    #[test]
    fn edge_a_two_word_keyword_is_offered_whole_from_its_first_word() {
        let completer = ReplCompleter::new();

        assert_eq!(
            completer
                .keywords
                .iter()
                .filter(|e| e.contains(' '))
                .count(),
            2,
            "the language's two-word keywords are the two the lexer gives one kind: {:?}",
            completer.keywords
        );
        for entry in completer.keywords.iter().filter(|e| e.contains(' ')) {
            let head = entry.split(' ').next().unwrap_or_default();
            assert!(
                completer.complete(head).contains(entry),
                "typing {head:?} offers the whole of {entry:?}, not one word of it"
            );
        }
    }

    /// `by` is the one word the completer offers that the lexer does not reserve,
    /// and it is offered because the parser reads it. So the offer is checked
    /// against programs: a stepped loop and a labelled argument are the two places
    /// the word appears, and both have to answer for the completion to be a
    /// promise rather than a guess.
    #[test]
    fn edge_the_one_non_reserved_keyword_is_a_word_the_parser_reads() {
        let completer = ReplCompleter::new();

        assert_eq!(
            POSITIONAL_KEYWORDS,
            &["by"],
            "a second word here would be a second exception with no rule behind it"
        );
        assert_eq!(
            printed("for each i from 0 to 10 by 5\n    say i\nend"),
            vec!["0", "5", "10"],
            "`by 5` is a stepped loop, which is one of the two places the word appears"
        );
        assert_eq!(
            printed("say split(\"a,b,c\", by \",\")"),
            vec!["[a, b, c]"],
            "`by \",\"` labels an argument, which is the other"
        );
        assert!(
            completer.complete("b").contains(&"by".to_string()),
            "so it is offered: {:?}",
            completer.complete("b")
        );
    }

    /// The command vocabulary is the parser's table, so every alias a user can
    /// type is offered: `:h` reaches `:help`, `:hist` and `:history`, which a
    /// hand-kept list of canonical names could never offer.
    #[test]
    fn complete_matches_a_repl_command() {
        let matches = ReplCompleter::new().complete(":h");

        assert!(
            matches.contains(&":help".to_string())
                && matches.contains(&":h".to_string())
                && matches.contains(&":history".to_string())
                && matches.contains(&":hist".to_string()),
            "an alias is offered as well as the canonical name: {:?}",
            matches
        );
    }

    /// `.` is the other sigil the parser accepts, and it used to be a spelling a
    /// user could type but could not complete: the completer held the words with
    /// `:` already in front of them, so `.q` finished to nothing and `.quit` was
    /// a word `:help` could not mention. Both sigils complete now, each back in
    /// the sigil the user typed.
    #[test]
    fn edge_completion_answers_to_the_sigil_the_user_typed() {
        let completer = ReplCompleter::new();

        let dotted = completer.complete(".q");
        assert!(
            dotted.contains(&".quit".to_string()) && dotted.contains(&".q".to_string()),
            ".q reaches the same table :q does: {:?}",
            dotted
        );
        assert_eq!(
            completer.complete(".loud"),
            completer.complete(":loud"),
            "only the sigil differs between the two"
        );
        for name in ReplCommand::names() {
            let dotted = format!("{}{}", commands::ALT_SIGIL, &name[1..]);
            assert!(
                completer.complete(&dotted).contains(&dotted),
                "{dotted:?} is what the parser accepts, so it is offered: {:?}",
                completer.complete(&dotted)
            );
        }
    }

    /// A word that is not a command word is not offered commands: `hi` is the
    /// start of `hist`, and a user typing it at a statement is not asking for
    /// `:hist`.
    #[test]
    fn edge_a_word_that_is_not_a_command_word_is_not_offered_one() {
        let matches = ReplCompleter::new().complete_with("hi", &session_names());

        assert!(
            !matches.iter().any(|match_| match_.starts_with(':')),
            "no sigil in the word, no command offered: {:?}",
            matches
        );
        assert!(
            !matches.iter().any(|match_| match_.starts_with('.')),
            "and neither sigil is offered behind a word that has none: {:?}",
            matches
        );
    }

    /// One table: every command word the completer offers is one
    /// `ReplCommand::parse` accepts, and vice versa. The completer used to keep
    /// its own list of sixteen names beside a parser that took thirty aliases, so
    /// nine working commands could not be completed.
    #[test]
    fn edge_the_command_vocabulary_is_the_parsers_own_table() {
        let completer = ReplCompleter::new();

        for name in ReplCommand::names() {
            assert!(
                completer.complete(&name).contains(&name),
                "{:?} parses, so it is offered: {:?}",
                name,
                completer.complete(&name)
            );
        }

        let offered = completer.complete(":");
        assert!(
            offered.len() == ReplCommand::names().len(),
            "and the completer offers nothing the parser does not accept: {:?}",
            offered
        );
    }

    /// Several vocabularies can answer the same prefix, and the result is one
    /// sorted list rather than four.
    #[test]
    fn complete_merges_every_vocabulary_into_one_sorted_list() {
        let completer = ReplCompleter::new();
        let matches = completer.complete_with("c", &session_names());

        assert!(
            matches.contains(&"counter".to_string())
                && matches.contains(&"circle_area".to_string()),
            "session names are offered: {:?}",
            matches
        );
        assert!(
            matches.contains(&"console".to_string()) && matches.contains(&"catch".to_string()),
            "builtins and keywords are offered alongside: {:?}",
            matches
        );

        let mut sorted = matches.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(matches, sorted, "results come back sorted and deduplicated");
    }

    #[test]
    fn complete_of_a_prefix_that_matches_nothing_is_empty() {
        let completer = ReplCompleter::new();

        assert!(
            completer.complete_with("zzzz", &session_names()).is_empty(),
            "no prefix means no matches, not every entry"
        );
        assert!(completer.complete("qzx").is_empty());
    }

    /// An empty prefix is "what could go here", so it offers the whole
    /// vocabulary — and offers it ordered, since the order is all the user has
    /// to go on.
    #[test]
    fn edge_empty_prefix_offers_the_whole_vocabulary_sorted() {
        let completer = ReplCompleter::new();
        let all = completer.complete("");

        assert!(
            all.len() > 50,
            "an empty prefix should offer more than a handful, got {}",
            all.len()
        );
        assert!(all.contains(&"set".to_string()), "keywords are offered");
        assert!(
            all.contains(&"uppercase".to_string()),
            "builtins are offered"
        );
        assert!(all.contains(&":quit".to_string()), "commands are offered");
        assert!(
            !all.contains(&"say \"hello\"".to_string()),
            "only vocabulary, not the session's own lines"
        );

        let mut sorted = all.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(all, sorted);
    }

    /// A session name that repeats a builtin is one completion, not two.
    #[test]
    fn edge_a_session_name_that_duplicates_a_builtin_appears_once() {
        let matches = ReplCompleter::new().complete_with("pi", &["PI".to_string()]);

        assert_eq!(matches, vec!["PI".to_string()]);
    }

    /// One entry per word in each table. The keyword list carried `to` twice, which
    /// only the `dedup` in `complete_with` hid — the table said the vocabulary had a
    /// word in it twice, and nothing could say so.
    #[test]
    fn edge_no_table_of_the_vocabulary_holds_a_word_twice() {
        let completer = ReplCompleter::new();

        for (label, words) in [
            ("keyword", &completer.keywords),
            ("builtin", &completer.builtins),
            ("command", &completer.commands),
        ] {
            let mut seen: Vec<&String> = Vec::new();
            for word in words {
                assert!(
                    !seen.contains(&word),
                    "the {} list holds {:?} more than once",
                    label,
                    word
                );
                seen.push(word);
            }
        }
    }

    /// The module names are the language's own table, and every one of them is
    /// offered.
    ///
    /// The completer kept its own list beside it, and it had drifted both ways: it
    /// offered `formats` — a name that has never been a module, which the analyzer
    /// refuses as `Unknown variable 'formats'` — and did not offer five modules
    /// that are in [`crate::stdlib::MODULES`] and do answer: `bytes`, `csv`,
    /// `json`, `sys` and `time`. The list is read from `MODULES` for the same
    /// reason the command list is read from [`commands::ReplCommand`]: a table
    /// written out again is a second answer to a question that already has one.
    #[test]
    fn edge_every_module_the_language_has_is_offered() {
        let completer = ReplCompleter::new();

        for module in crate::stdlib::MODULES {
            let offered = completer.complete(module);
            assert!(
                offered.contains(&(*module).to_string()),
                "{module} is a module, so a user typing it is offered it: {offered:?}"
            );
        }
    }

    /// Every builtin the completer offers is a name a program can actually call
    /// **and have answered**.
    ///
    /// Tab-completion is the one place the language *invites* a name rather than
    /// waiting to be asked, so a name here is a promise — and this table held six
    /// it could not keep. `formats` is the one this phase is about (SPEC.md and
    /// README.md both stopped naming it in rounds 1 and 2; the completer still
    /// offered it), and `parse_json`, `to_json`, `read`, `write` and `exists` are
    /// the same fiction flat-spelled: each answers `Unknown function`, because
    /// JSON is `json.parse` and the file functions are `files.read`.
    ///
    /// The check is [`crate::stdlib::flat_builtin_names`] rather than
    /// [`crate::stdlib::is_builtin`], and that difference is this gate's second
    /// life. Registration was the whole of the old predicate, so `filter` and
    /// `reduce` passed it while every call to either of them is a refusal —
    /// the gate was green through the exact defect it exists to prevent, because
    /// a registered name is spelled correctly and nothing more.
    #[test]
    fn edge_the_repl_offers_no_builtin_a_program_cannot_call() {
        let completer = ReplCompleter::new();
        let flat = crate::stdlib::flat_builtin_names();

        for name in &completer.builtins {
            if crate::stdlib::is_module(name) || crate::stdlib::is_global(name) {
                continue;
            }
            assert!(
                !crate::stdlib::is_unimplemented(name),
                "{name} is registered and nothing implements it, so it is not a name to \
                 offer: every call to it is a refusal, and a completion is a promise \
                 that the call will answer"
            );
            assert!(
                flat.contains(&name.as_str()),
                "{name} is offered as a builtin, so it has to be one a program can name: \
                 it is not a module, not a global constant, and not one of the flat \
                 builtins that answer"
            );
        }
    }

    /// The names the completer leaves out because nothing implements them are
    /// names no program can call — and the programs are run here, in both
    /// directions, so the exclusion cannot be satisfied by a name that starts
    /// working.
    ///
    /// The other half of
    /// [`edge_the_repl_offers_no_builtin_a_program_cannot_call`], and the half it
    /// cannot see: that gate walks what is offered, and a name is *absent* from a
    /// list rather than wrong, so it stayed green while `filter` and `reduce` were
    /// offered. This walks [`crate::stdlib::UNIMPLEMENTED_BUILTINS`] instead and
    /// asks the language, not the table.
    ///
    /// The last assertion is the one the table cannot make on its own, and the
    /// others are the table's own opinion restated so it has to agree with the
    /// language. `edge_a_known_gap_is_still_a_gap`
    /// (`tests/stdlib_module_docs_test.rs`) runs the same programs under the CLI
    /// and is what forces an implemented gap out of the list; running them here as
    /// well is what makes the *completion* side of that decision falsifiable in the
    /// same process as the completer that has to keep it.
    #[test]
    fn edge_no_unimplemented_builtin_is_offered_and_each_one_refuses() {
        let completer = ReplCompleter::new();
        assert!(
            !crate::stdlib::UNIMPLEMENTED_BUILTINS.is_empty(),
            "the registry still holds builtins nothing implements, so this is checking \
             something"
        );

        for name in crate::stdlib::UNIMPLEMENTED_BUILTINS {
            assert!(
                crate::stdlib::is_builtin(name),
                "{name} is listed as unimplemented and is not even a registered builtin, \
                 so the entry is left over from a name that is gone"
            );
            assert!(
                !crate::stdlib::flat_builtin_names().contains(name),
                "{name} answers nothing, so it is not a name a program may write flat"
            );
            assert!(
                !completer.builtins.contains(&name.to_string()),
                "{name} answers nothing, so the completer does not offer it: {:?}",
                completer.builtins
            );
            assert!(
                completer.complete(name).is_empty(),
                "{name} completes to nothing rather than to itself: {:?}",
                completer.complete(name)
            );
            let program = crate::stdlib::unimplemented_program(name).unwrap_or_else(|| {
                panic!(
                    "{name} is listed as unimplemented and has no program in \
                     stdlib::UNIMPLEMENTED_BUILTIN_PROGRAMS, so nothing runs it and the \
                     claim cannot expire on its own"
                )
            });
            let refusal = refuses(program);
            assert!(
                refusal.contains(name) && !refusal.contains("Unknown function"),
                "{name} is not offered because a call to it is a refusal naming itself, \
                 so the offer and the call are the same claim. If this name now answers, \
                 implement it and take it out of stdlib::UNIMPLEMENTED_BUILTINS: {refusal}"
            );
        }
    }

    /// The program a gap is refused by has to be **the program the language
    /// documents**, and the whole of this gate is about that.
    ///
    /// The gate above used to run `say {name}([1, 2])` — one argument, where the
    /// function is missing — which is a refusal for a reason that has nothing to
    /// do with whether the function exists: `filter` is refused there because two
    /// arguments are not one, and an implemented `filter` would be refused in
    /// exactly the same words. So implementing `filter` without touching this
    /// gate left it green, and the name would still be listed as a gap while
    /// answering. The same shape was in the public-surface twin.
    ///
    /// What makes the difference is the argument list, not the wording of the
    /// assertion, so the programs live in
    /// [`crate::stdlib::UNIMPLEMENTED_BUILTIN_PROGRAMS`] beside the names they
    /// belong to: `filter([1, 2, 3], to (x) give back x > 1 end)` is the shape a
    /// program writes, and an implemented `filter` answers it. The table is the
    /// one `known_gaps()` in `tests/stdlib_module_docs_test.rs` holds its own
    /// copy of, and the two are gated on being the same, so the copies cannot
    /// drift into checking different things.
    ///
    /// Two properties of the programs are asserted rather than assumed, because a
    /// program that stops being the documented one is how the gate above goes
    /// quiet again: it has to be a call to the name it is filed under, and it
    /// has to pass the arguments that name takes.
    #[test]
    fn edge_a_gap_program_is_the_call_the_language_documents_and_not_a_short_one() {
        assert_eq!(
            crate::stdlib::UNIMPLEMENTED_BUILTIN_PROGRAMS
                .iter()
                .map(|(name, _)| *name)
                .collect::<Vec<&str>>(),
            crate::stdlib::UNIMPLEMENTED_BUILTINS.to_vec(),
            "a gap listed without a program, or a program filed under a name that is not a \
             gap, so one of them is never run"
        );

        for (name, program) in crate::stdlib::UNIMPLEMENTED_BUILTIN_PROGRAMS {
            let call = program
                .split_once(&format!("{name}("))
                .unwrap_or_else(|| panic!("`{program}` does not call {name}"))
                .1;
            let arguments = call
                .split_once(')')
                .unwrap_or_else(|| panic!("`{program}` writes no closing paren"))
                .0;
            assert!(
                arguments.contains('('),
                "`{program}` calls {name} with one argument, and one argument is refused for \
                 its arity rather than for the function being missing: an implemented {name} \
                 is refused in the same words, so this program cannot tell the two apart"
            );
            let refusal = refuses(program);
            assert!(
                refusal.contains(name),
                "`{program}` was refused without naming {name}: {refusal}"
            );
        }
    }

    /// Every registered builtin the flat table drops is a name a program cannot
    /// call without a receiver — so the table's name is true.
    ///
    /// [`crate::stdlib::flat_builtin_names`] is documented as the names a person
    /// may type without a module in front of them, and that was half a claim
    /// until the language said it: the twenty-five `<module>_<member>` encodings
    /// it withholds *answered* when written flat, so a program could write every
    /// name the table excluded and the REPL offered none of them.
    ///
    /// This is the property in one walk, and it is the same walk for both kinds
    /// of name, so a third kind cannot appear without being noticed: every name
    /// in [`crate::stdlib::BUILTIN_NAMES`] outside the flat table is either an
    /// encoding — refused by
    /// [`crate::stdlib::encoding_written_without_a_receiver`] — or a gap, which
    /// the gate above runs in the shape a program writes it. Every one of them is
    /// also absent from the offer, which is the half a user sees.
    #[test]
    fn edge_every_registered_builtin_outside_the_flat_table_is_refused_flat() {
        let completer = ReplCompleter::new();
        let flat = crate::stdlib::flat_builtin_names();
        let withheld: Vec<&str> = crate::stdlib::BUILTIN_NAMES
            .iter()
            .copied()
            .filter(|name| !flat.contains(name))
            .collect();
        assert!(
            !withheld.is_empty(),
            "the registry holds builtins no program may write flat, so this is checking \
             something"
        );

        for name in withheld {
            assert!(
                crate::stdlib::is_module_member_encoding(name)
                    || crate::stdlib::is_unimplemented(name),
                "{name} is registered and is in neither flat_builtin_names nor \
                 UNIMPLEMENTED_BUILTINS, so it is a third kind of name the flat table \
                 withholds without the language refusing it — and the table's name would \
                 be false about it"
            );
            assert!(
                !completer.builtins.contains(&name.to_string()),
                "{name} is not a name a program may write flat, so it is not offered: {:?}",
                completer.builtins
            );

            let refusal = refuses(&format!("say {name}()"));
            assert!(
                refusal.contains(name),
                "{name} is not offered, so a program writing it must be refused by a \
                 message naming it: {refusal}"
            );
            if crate::stdlib::is_module_member_encoding(name) {
                let (module, member) = name
                    .split_once('_')
                    .expect("an encoding has a module in it");
                assert!(
                    refusal.contains(&format!("{module}.{member}")),
                    "{name} is refused as an encoding, so the refusal has to name the \
                     spelling that works, {module}.{member}: {refusal}"
                );
            }
        }
    }

    /// The other direction, and the one the gate above cannot see: every name the
    /// language has is offered.
    ///
    /// `edge_the_repl_offers_no_builtin_a_program_cannot_call` walks the table and
    /// asks whether each entry is real, which is only half of a promise — it is
    /// blind to a builtin that answers and is not on the list, which is how
    /// twenty-one of them were missing. A user typing `log`, `to_text`, `type_of`,
    /// `input` or `starts_with` got nothing, and the gate was green the whole time,
    /// because nothing here was *wrong*, only absent.
    ///
    /// So the list is walked the other way: [`crate::stdlib::flat_builtin_names`]
    /// and [`crate::stdlib::GLOBAL_CONSTANTS`] are the whole of what a program can
    /// write without a receiver in front of it **and have answered**, and every one
    /// of them completes.
    ///
    /// "and have answered" is what the table now filters on, and it is the second
    /// half of the promise the gate above cannot see: a builtin that answers is
    /// *absent* from a list rather than wrong in it. The two names the registry
    /// holds and nothing dispatches are excluded by the same filter that keeps
    /// this gate and the offer in step — see
    /// [`edge_no_unimplemented_builtin_is_offered_and_each_one_refuses`].
    #[test]
    fn edge_every_flat_builtin_the_language_has_is_offered() {
        let completer = ReplCompleter::new();
        let expected = crate::stdlib::flat_builtin_names()
            .into_iter()
            .chain(crate::stdlib::GLOBAL_CONSTANTS.iter().copied())
            .collect::<Vec<&str>>();

        for name in expected {
            let offered = completer.complete(name);
            assert!(
                offered.contains(&name.to_string()),
                "{name} is a builtin a program can write flat, so typing it is offered it: \
                 {offered:?}"
            );
        }
    }

    /// The names the registry holds *because* of a module are not offered flat.
    ///
    /// `files_read` is how `files.read` is stored and it is a registered builtin, so
    /// the gate above has to pass it — but no document writes it and no user should
    /// be handed it. Offering an internal encoding teaches the one spelling the
    /// language refuses, which is the `read`/`write`/`exists` defect of
    /// `edge_the_repl_offers_no_builtin_a_program_cannot_call` one layer down.
    ///
    /// Withholding it used to be a judgement about what to hand a user rather than
    /// a statement about the language — `say files_read(path)` answered on both
    /// engines — so the offer and the language disagreed and the only honest
    /// reading of this gate was "we are hiding a working name". The language now
    /// refuses the encoding where a program writes it without a receiver, naming
    /// the dotted spelling, so withholding it and offering it would each be wrong
    /// in the same direction. The refusal is
    /// `edge_every_registered_builtin_outside_the_flat_table_is_refused_flat`'s
    /// job; this one holds the offer, and the dotted spelling that replaces it is
    /// `edge_every_dropped_encoding_is_reachable_through_the_module_offered`'s.
    ///
    /// Read from [`crate::stdlib::BUILTIN_NAMES`] rather than from a list, so
    /// this covers every encoding there is — twenty-five of them — and a new module
    /// function is covered without a line changing here.
    #[test]
    fn edge_no_module_function_is_offered_under_its_registry_encoding() {
        let completer = ReplCompleter::new();
        let encodings = crate::stdlib::BUILTIN_NAMES
            .iter()
            .copied()
            .filter(|name| crate::stdlib::is_module_member_encoding(name))
            .collect::<Vec<&str>>();
        assert!(
            !encodings.is_empty(),
            "the registry still holds module functions, so this is checking something"
        );

        for name in encodings {
            assert!(
                !completer.builtins.contains(&name.to_string()),
                "{name} is the registry's encoding of a module function, and a program \
                 writes it dotted, so it is not offered flat"
            );
            assert!(
                completer.complete(name).is_empty(),
                "{name} completes to nothing rather than to itself: {:?}",
                completer.complete(name)
            );
        }
    }

    /// Every module function is still reachable under the spelling the completer
    /// *does* offer — the module — so dropping the encodings above dropped no
    /// name a user could type. `files.read` needs `files` to be offered and the
    /// member to be one the language has.
    #[test]
    fn edge_every_dropped_encoding_is_reachable_through_the_module_offered() {
        let completer = ReplCompleter::new();

        for name in crate::stdlib::BUILTIN_NAMES {
            let Some((module, member)) = name.split_once('_') else {
                continue;
            };
            if !crate::stdlib::is_module_member_encoding(name) {
                continue;
            }
            assert!(
                completer.complete(module).contains(&module.to_string()),
                "{name} is reached as {module}.{member}, so the module is what a user \
                 types first and it has to be offered: {:?}",
                completer.complete(module)
            );
            assert!(
                crate::stdlib::module_has_member(module, member),
                "{name} is dropped as an encoding, so {module}.{member} is the spelling \
                 that has to hold, and the member table is what says it does"
            );
        }
    }

    /// The half of that claim the table cannot make: the dotted spelling is
    /// *reached*, on the engine the offer is made in.
    ///
    /// Refusing an encoding is only safe because `files.read(..)` still answers,
    /// so a refusal the language hands a user replaces a spelling that works and
    /// not one that was already gone. The two paths are told apart by whether the
    /// program wrote a receiver — `Expr::Call` against `CallMethod` here, `CALL`
    /// against `CALL_METHOD` in the bytecode VM — so a mistake in that test would
    /// break the dotted spelling while every gate about the *offer* stayed green:
    /// those walk tables, and the tables do not know which path a call takes.
    ///
    /// So the dotted spelling is run, with no arguments. The arguments are not
    /// the point and cannot be invented for twenty-five functions; what has to
    /// hold is that the call reaches the resolver instead of being turned away at
    /// the door, which is the one thing the encoding refusal could break. So the
    /// assertion is on the message: a refusal here has to be about the arguments
    /// the function was given, never the encoding rule.
    /// `edge_a_module_function_is_offered_as_its_module_and_not_as_its_encoding` in
    /// `tests/repl_module_completion_test.rs` runs the same walk on the second
    /// engine.
    #[test]
    fn edge_the_dotted_spelling_of_an_encoding_still_reaches_the_function() {
        for name in crate::stdlib::BUILTIN_NAMES {
            if !crate::stdlib::is_module_member_encoding(name) {
                continue;
            }
            let (module, member) = name
                .split_once('_')
                .expect("an encoding has a module in it");

            if let Err(refusal) = run(&format!("say {module}.{member}()")) {
                assert!(
                    !refusal.contains("not a name on its own"),
                    "{module}.{member} was refused by the rule that refuses the encoding \
                     without a receiver, so the two paths are no longer told apart and the \
                     dotted spelling is gone: {refusal}"
                );
            }
        }
    }

    /// Completion is offered on what the user typed, not on how they cased it.
    #[test]
    fn edge_completion_ignores_the_case_of_the_prefix() {
        let completer = ReplCompleter::new();

        assert_eq!(completer.complete("UP"), completer.complete("up"));
        assert!(
            completer
                .complete_with("math", &session_names())
                .contains(&"MathUtils".to_string()),
            "the original spelling of the name is what is offered"
        );
    }
}
