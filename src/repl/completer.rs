use super::commands;

pub struct ReplCompleter {
    keywords: Vec<String>,
    builtins: Vec<String>,
    commands: Vec<String>,
}

impl ReplCompleter {
    pub fn new() -> Self {
        Self {
            keywords: vec![
                "set".to_string(),
                "to".to_string(),
                "is".to_string(),
                "are".to_string(),
                "if".to_string(),
                "then".to_string(),
                "else".to_string(),
                "end".to_string(),
                "when".to_string(),
                "unless".to_string(),
                "for".to_string(),
                "each".to_string(),
                "in".to_string(),
                "from".to_string(),
                "by".to_string(),
                "times".to_string(),
                "while".to_string(),
                "repeat".to_string(),
                "until".to_string(),
                "break".to_string(),
                "skip".to_string(),
                "return".to_string(),
                "give back".to_string(),
                "say".to_string(),
                "print".to_string(),
                "ask".to_string(),
                "try".to_string(),
                "catch".to_string(),
                "finally".to_string(),
                "and".to_string(),
                "or".to_string(),
                "not".to_string(),
                "mod".to_string(),
                "yes".to_string(),
                "no".to_string(),
                "nothing".to_string(),
                "module".to_string(),
                "import".to_string(),
                "export".to_string(),
                "object".to_string(),
                "has".to_string(),
                "can".to_string(),
                "this".to_string(),
                "new".to_string(),
                "extends".to_string(),
                "async".to_string(),
                "wait".to_string(),
                "parallel".to_string(),
                "done".to_string(),
            ],
            builtins: vec![
                "math".to_string(),
                "text".to_string(),
                "files".to_string(),
                "network".to_string(),
                "formats".to_string(),
                "list".to_string(),
                "console".to_string(),
                "PI".to_string(),
                "E".to_string(),
                "abs".to_string(),
                "floor".to_string(),
                "ceil".to_string(),
                "round".to_string(),
                "sqrt".to_string(),
                "pow".to_string(),
                "sin".to_string(),
                "cos".to_string(),
                "tan".to_string(),
                "length".to_string(),
                "push".to_string(),
                "pop".to_string(),
                "map".to_string(),
                "filter".to_string(),
                "reduce".to_string(),
                "random".to_string(),
                "uppercase".to_string(),
                "lowercase".to_string(),
                "trim".to_string(),
                "split".to_string(),
                "join".to_string(),
                "contains".to_string(),
                "read".to_string(),
                "write".to_string(),
                "exists".to_string(),
                "parse_json".to_string(),
                "to_json".to_string(),
            ],
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
    use super::ReplCompleter;
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
