#[derive(Debug, PartialEq, Eq)]
pub enum ReplCommand {
    /// A line that is Redblue source rather than a command.
    ///
    /// This is not [`ReplCommand::Unknown`]: `say "hi"` is not a mistyped
    /// command, it is a program. Conflating the two made the REPL answer
    /// `2 + 3` with "Unknown command '2 + 3'" and refuse to evaluate it.
    NotACommand,
    Quit,
    Help,
    Clear,
    History,
    Vars,
    Functions,
    Load(String),
    Save(String),
    /// Reads a history file written by [`ReplCommand::Save`] back into the
    /// session's history.
    ///
    /// The other half of `:save`, which had no half: a saved file could not be
    /// loaded by any REPL path, so the round trip existed only in unit tests.
    Restore(String),
    Reset,
    Run(String),
    Debug,
    Inspect(String),
    Ast(String),
    Tokens(String),
    Example,
    /// A command that was recognised but not given what it needs.
    ///
    /// Not [`ReplCommand::Unknown`]: the command *is* known, its argument is
    /// not, and reporting "Unknown command 'load requires a file path'" told
    /// the user they had mistyped a command they had typed correctly.
    MissingArgument {
        command: &'static str,
        usage: &'static str,
    },
    Unknown(String),
}

/// One command in the table: every word that reaches it, what it does, the usage
/// shown when it was given no argument, and how it builds itself.
///
/// The table is the single authority on what a command is called, what it does
/// and what it takes. It replaced a hand-written `match` plus a separate list in
/// the completer plus a third written-out list in the help text, which is how
/// `:q`, `:h`, `:hist`, `:v` and `:f` came to parse without being completable.
struct CommandSpec {
    /// Every name that reaches this command, canonical name first — the name
    /// [`ReplCommand::MissingArgument`] reports.
    names: &'static [&'static str],
    /// What the command is for, printed beside its name by `:help`.
    description: &'static str,
    /// What the command does with its argument, and the placeholder `:help` and
    /// the missing-argument report both show. `None` for a command that takes
    /// none, which is also what tells the parser it must not be given one.
    usage: Option<&'static str>,
    build: fn(Option<String>) -> ReplCommand,
}

/// The sigil other than `:`.
///
/// `:quit` and `.quit` are the same command. Named rather than read out of
/// [`SIGILS`] by index, because `:help` prints it and an index into a
/// two-element array is a way to print `:` twice.
pub const ALT_SIGIL: char = '.';

/// Every sigil a command word may begin with.
///
/// A command word carries exactly one. `:quit` and `.quit` are the same command,
/// and `::quit` is not a command at all: it is reported as the unknown word
/// `":quit"`. Stripping every leading sigil made `::load x` and `...quit` reach
/// the commands inside them, so a typo loaded a file or ended the session.
pub const SIGILS: [char; 2] = [':', ALT_SIGIL];

/// Every command, in the order `:help` lists them.
static COMMAND_TABLE: &[CommandSpec] = &[
    CommandSpec {
        names: &["quit", "exit", "q"],
        description: "Exit the REPL",
        usage: None,
        build: |_| ReplCommand::Quit,
    },
    CommandSpec {
        names: &["help", "h", "?"],
        description: "Show this help",
        usage: None,
        build: |_| ReplCommand::Help,
    },
    CommandSpec {
        names: &["clear", "cls"],
        description: "Clear the screen",
        usage: None,
        build: |_| ReplCommand::Clear,
    },
    CommandSpec {
        names: &["history", "hist"],
        description: "Show command history",
        usage: None,
        build: |_| ReplCommand::History,
    },
    CommandSpec {
        names: &["vars", "variables", "v"],
        description: "Show all variables",
        usage: None,
        build: |_| ReplCommand::Vars,
    },
    CommandSpec {
        names: &["functions", "funcs", "f"],
        description: "Show defined functions",
        usage: None,
        build: |_| ReplCommand::Functions,
    },
    CommandSpec {
        names: &["load", "l"],
        description: "Load and run a file",
        usage: Some("<filename>"),
        build: |argument| ReplCommand::Load(argument.unwrap_or_default()),
    },
    CommandSpec {
        names: &["save", "s"],
        description: "Save current session",
        usage: Some("<filename>"),
        build: |argument| ReplCommand::Save(argument.unwrap_or_default()),
    },
    CommandSpec {
        names: &["restore", "r"],
        description: "Restore a saved session",
        usage: Some("<filename>"),
        build: |argument| ReplCommand::Restore(argument.unwrap_or_default()),
    },
    CommandSpec {
        names: &["reset"],
        description: "Reset REPL state",
        usage: None,
        build: |_| ReplCommand::Reset,
    },
    CommandSpec {
        names: &["run"],
        description: "Run a Redblue file",
        usage: Some("<filename>"),
        build: |argument| ReplCommand::Run(argument.unwrap_or_default()),
    },
    CommandSpec {
        names: &["debug"],
        description: "Debug mode",
        usage: None,
        build: |_| ReplCommand::Debug,
    },
    CommandSpec {
        names: &["inspect", "i"],
        description: "Inspect a variable",
        usage: Some("<variable>"),
        build: |argument| ReplCommand::Inspect(argument.unwrap_or_default()),
    },
    CommandSpec {
        names: &["ast"],
        description: "Print AST for expression",
        usage: Some("<expression>"),
        build: |argument| ReplCommand::Ast(argument.unwrap_or_default()),
    },
    CommandSpec {
        names: &["tokens"],
        description: "Print tokens for expression",
        usage: Some("<expression>"),
        build: |argument| ReplCommand::Tokens(argument.unwrap_or_default()),
    },
    CommandSpec {
        names: &["example", "examples"],
        description: "Show example code",
        usage: None,
        build: |_| ReplCommand::Example,
    },
];

impl ReplCommand {
    /// Every command word the table accepts, without a sigil, sorted.
    ///
    /// The word itself rather than a spelling of it: which sigil the user types
    /// is theirs, and [`SIGILS`] are all accepted, so the completer matches what
    /// they typed against this and puts their sigil back.
    ///
    /// Not deduplicated: a word the table holds twice is a word two commands
    /// answer to, and [`bare_names`] reporting it twice is what lets a test say
    /// so. The completer's own `dedup` hides it from a user either way.
    pub fn bare_names() -> Vec<String> {
        let mut names: Vec<String> = COMMAND_TABLE
            .iter()
            .flat_map(|spec| spec.names.iter())
            .map(|name| (*name).to_string())
            .collect();
        names.sort();
        names
    }

    /// Every command word the table accepts, canonical sigil and all, sorted.
    ///
    /// Completion offers exactly these. The completer used to carry its own
    /// 16-entry list of canonical names while the table accepted thirty-odd
    /// aliases, so `:q`, `:h`, `:hist`, `:v` and `:f` all worked and none of
    /// them were completable — the same two-table divergence this table exists
    /// to delete, one level down.
    ///
    /// The `:` spelling is the one offered and the one `:help` prints; the other
    /// sigil in [`SIGILS`] is the same word and is completed by
    /// [`ReplCompleter`](super::ReplCompleter) when the user types it.
    pub fn names() -> Vec<String> {
        let mut names: Vec<String> = ReplCommand::bare_names()
            .into_iter()
            .map(|name| format!(":{}", name))
            .collect();
        names.sort();
        names
    }

    /// One line per command, aliases included, for `:help`.
    ///
    /// Built from [`COMMAND_TABLE`], aliases and all, so every word the parser
    /// accepts is one the user can read about, and a command added to the table
    /// cannot be missing from the help. Padded to a common column so the
    /// descriptions line up however long the command and its argument are.
    pub fn help_lines() -> Vec<String> {
        let entries: Vec<(String, &'static str)> = COMMAND_TABLE
            .iter()
            .map(|spec| {
                let aliases: Vec<String> =
                    spec.names.iter().map(|name| format!(":{}", name)).collect();
                let signature = match spec.usage {
                    Some(usage) => format!("{} {}", aliases.join(", "), usage),
                    None => aliases.join(", "),
                };
                (signature, spec.description)
            })
            .collect();

        let width = entries
            .iter()
            .map(|(signature, _)| signature.len())
            .max()
            .unwrap_or(0)
            + 2;

        entries
            .into_iter()
            .map(|(signature, description)| {
                format!("  {:width$}  {}", signature, description, width = width)
            })
            .collect()
    }

    /// The whole of a command's argument: every word after the command word, not
    /// just the first.
    ///
    /// One word was not enough for any of the seven commands that take one.
    /// `:ast 1 + 1` became `Ast("1")`, so the AST it printed was of `1`, and
    /// `:load /tmp/a b.rb` became `Load("/tmp/a")` — a path silently truncated
    /// to a directory that is not the file the user named.
    ///
    /// The remainder is sliced out of `input` rather than rebuilt from words
    /// joined with a single space. Re-joining collapsed the whitespace inside
    /// it: `:ast say "a   b"` became `say "a b"`, and a path with two spaces in
    /// it named a file that does not exist. The whitespace around the argument
    /// is the separator and is dropped; the whitespace inside it is the user's
    /// and is kept.
    fn argument(input: &str) -> Option<String> {
        let (_, rest) = input.split_once(char::is_whitespace)?;
        let argument = rest.trim_start();
        if argument.is_empty() {
            None
        } else {
            Some(argument.to_string())
        }
    }

    pub fn parse(input: &str) -> Self {
        let input = input.trim();

        if !input.starts_with(SIGILS) {
            return ReplCommand::NotACommand;
        }

        let word = match input.split_once(char::is_whitespace) {
            Some((word, _)) => word,
            None => input,
        };
        // Exactly one sigil, so `::quit` and `...quit` name nothing rather than
        // reaching the command one sigil in.
        let command = word.strip_prefix(SIGILS).unwrap_or(word);

        match COMMAND_TABLE
            .iter()
            .find(|spec| spec.names.contains(&command))
        {
            Some(spec) => match spec.usage {
                None => (spec.build)(None),
                Some(usage) => match Self::argument(input) {
                    Some(value) => (spec.build)(Some(value)),
                    None => ReplCommand::MissingArgument {
                        command: spec.names[0],
                        usage,
                    },
                },
            },
            None => ReplCommand::Unknown(command.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ReplCommand;

    /// Every alias the help text and the completer promise, what it must parse
    /// to, and — for the commands that take one — the argument it must carry.
    ///
    /// This table is the point of the module: when the REPL carried a second
    /// copy of it, an alias added here was invisible to the REPL. A row that
    /// stops parsing is a regression, not a rename.
    ///
    /// The argument is part of the assertion because checking only the variant
    /// could not fail: `:ast 1 + 1` parsed to `Ast("1")` and this table called
    /// that a pass. Each row is `(input, variant, argument)`, with an empty
    /// argument for a command that takes none.
    #[test]
    fn parse_reads_the_whole_alias_table() {
        let cases: Vec<(&str, &str, &str)> = vec![
            (":quit", "Quit", ""),
            (":exit", "Quit", ""),
            (":q", "Quit", ""),
            (":help", "Help", ""),
            (":h", "Help", ""),
            (":?", "Help", ""),
            (":clear", "Clear", ""),
            (":cls", "Clear", ""),
            (":history", "History", ""),
            (":hist", "History", ""),
            (":vars", "Vars", ""),
            (":variables", "Vars", ""),
            (":v", "Vars", ""),
            (":functions", "Functions", ""),
            (":funcs", "Functions", ""),
            (":f", "Functions", ""),
            (":load some.rb", "Load", "some.rb"),
            (":l some.rb", "Load", "some.rb"),
            (":save out.txt", "Save", "out.txt"),
            (":s out.txt", "Save", "out.txt"),
            (":restore out.txt", "Restore", "out.txt"),
            (":r out.txt", "Restore", "out.txt"),
            (":reset", "Reset", ""),
            (":run prog.rb", "Run", "prog.rb"),
            (":debug", "Debug", ""),
            (":inspect x", "Inspect", "x"),
            (":i x", "Inspect", "x"),
            (":ast 1 + 1", "Ast", "1 + 1"),
            (":tokens 1 + 1", "Tokens", "1 + 1"),
            (":example", "Example", ""),
            (":examples", "Example", ""),
        ];

        for (input, expected, argument) in cases {
            let parsed = ReplCommand::parse(input);
            let rendered = format!("{:?}", parsed);
            let variant = rendered.split(['(', ' ']).next().unwrap_or_default();
            assert_eq!(
                variant, expected,
                "parsing {:?} should give {}",
                input, expected
            );

            let carried = match &parsed {
                ReplCommand::Load(value)
                | ReplCommand::Save(value)
                | ReplCommand::Restore(value)
                | ReplCommand::Run(value)
                | ReplCommand::Inspect(value)
                | ReplCommand::Ast(value)
                | ReplCommand::Tokens(value) => value.as_str(),
                _ => "",
            };
            assert_eq!(
                carried, argument,
                "parsing {:?} should carry the argument {:?}",
                input, argument
            );
        }
    }

    /// A command's argument is every word after it, not the first one. Reading
    /// one word truncated a path to a directory that is not the file the user
    /// named and printed the AST of `1` for `:ast 1 + 1`.
    #[test]
    fn parse_reads_the_argument_a_command_was_given() {
        assert_eq!(
            ReplCommand::parse(":load /tmp/a b.rb"),
            ReplCommand::Load("/tmp/a b.rb".to_string()),
            "a path with a space in it is the whole remainder"
        );
        assert_eq!(
            ReplCommand::parse(":run /tmp/my program.rb"),
            ReplCommand::Run("/tmp/my program.rb".to_string())
        );
        assert_eq!(
            ReplCommand::parse(":ast 1 + 1"),
            ReplCommand::Ast("1 + 1".to_string()),
            "an expression is not its first word"
        );
        assert_eq!(
            ReplCommand::parse(":tokens set x to 1 + 2"),
            ReplCommand::Tokens("set x to 1 + 2".to_string())
        );
        assert_eq!(
            ReplCommand::parse("  :inspect counter  "),
            ReplCommand::Inspect("counter".to_string()),
            "the surrounding whitespace is still not part of the argument"
        );
        assert_eq!(
            ReplCommand::parse(".quit"),
            ReplCommand::Quit,
            "a dot is the other command sigil"
        );
    }

    /// Whitespace *inside* an argument belongs to the argument. Rebuilding it
    /// from words joined by one space collapsed it, so `:ast say "a   b"` asked
    /// for the AST of `say "a b"` and a path with two spaces in it named a file
    /// that does not exist.
    #[test]
    fn edge_internal_spacing_in_an_argument_is_kept() {
        assert_eq!(
            ReplCommand::parse(r#":ast   say "a   b""#),
            ReplCommand::Ast(r#"say "a   b""#.to_string()),
            "the runs of spaces inside the string are the user's, not ours"
        );
        assert_eq!(
            ReplCommand::parse(":load   /tmp/a  b.rb  "),
            ReplCommand::Load("/tmp/a  b.rb".to_string()),
            "leading space is the separator and is dropped; the two inside are not"
        );
        assert_eq!(
            ReplCommand::parse(":tokens   1   +   1"),
            ReplCommand::Tokens("1   +   1".to_string())
        );
    }

    /// The completer offers what [`ReplCommand::names`] returns, and that is the
    /// table itself: an alias that parses and is not offered is the divergence
    /// that shipped, where `:q`, `:h`, `:hist`, `:v`, `:f`, `:l`, `:s`, `:i` and
    /// `:examples` all worked and none of them could be completed.
    #[test]
    fn names_offers_every_alias_the_table_accepts() {
        for name in ReplCommand::names() {
            let with_argument = format!("{} example", name);
            assert!(
                !matches!(ReplCommand::parse(&name), ReplCommand::Unknown(_)),
                "{:?} is offered, so it must parse",
                name
            );
            assert!(
                !matches!(
                    ReplCommand::parse(&with_argument),
                    ReplCommand::Unknown(_) | ReplCommand::NotACommand
                ),
                "{:?} is offered with an argument, so it must parse with one",
                name
            );
        }
    }

    /// Every alias in the table is listed, under the sigil the user types. A
    /// table entry with no name here would be a word the REPL answers to and the
    /// help cannot mention.
    #[test]
    fn edge_help_lists_every_alias_of_every_command() {
        let help = ReplCommand::help_lines().join("\n");

        for name in ReplCommand::names() {
            assert!(
                help.contains(&name),
                "{:?} parses, so :help has to mention it:\n{}",
                name,
                help
            );
        }
    }

    /// A line of help is a name, its argument and what it does — and nothing of
    /// it is written twice, so a command cannot be listed under a spelling the
    /// parser does not accept.
    #[test]
    fn edge_help_has_one_line_per_command() {
        let help = ReplCommand::help_lines();

        assert_eq!(
            help.len(),
            super::COMMAND_TABLE.len(),
            "one line per command, not one per alias and not one for a repeat"
        );
        for line in &help {
            assert!(
                line.starts_with("  :") && line.trim().len() > 4,
                "a help line names its command: {:?}",
                line
            );
        }
    }

    /// Plain source is not a mistyped command. Conflating the two made the REPL
    /// answer `2 + 3` with "Unknown command" and never evaluate it.
    #[test]
    fn parse_of_plain_source_is_not_a_command() {
        for line in [
            "2 + 3",
            "say \"hi\"",
            "set x to 1",
            "if 1 is greater than 0 then",
        ] {
            assert_eq!(
                ReplCommand::parse(line),
                ReplCommand::NotACommand,
                "{:?} is source, not a command",
                line
            );
        }
    }

    /// A recognised command with nothing to work on is reported as missing its
    /// argument, not as an unknown command: the user typed the command correctly.
    #[test]
    fn parse_of_a_command_with_no_argument_says_what_is_missing() {
        assert_eq!(
            ReplCommand::parse(":load"),
            ReplCommand::MissingArgument {
                command: "load",
                usage: "<filename>"
            }
        );
        assert_eq!(
            ReplCommand::parse(":inspect"),
            ReplCommand::MissingArgument {
                command: "inspect",
                usage: "<variable>"
            }
        );
        assert_eq!(
            ReplCommand::parse(":tokens"),
            ReplCommand::MissingArgument {
                command: "tokens",
                usage: "<expression>"
            }
        );
        assert_eq!(
            ReplCommand::parse(":bogus"),
            ReplCommand::Unknown("bogus".to_string()),
            "a command that does not exist is still unknown"
        );
    }

    #[test]
    fn edge_empty_and_whitespace_input_is_not_a_command() {
        for line in ["", "   ", "\t"] {
            assert_eq!(
                ReplCommand::parse(line),
                ReplCommand::NotACommand,
                "{:?} should not name a command",
                line
            );
        }
    }

    /// A bare sigil is a command word with no name, not an empty program: it is
    /// reported, and it names nothing rather than panicking on an empty word.
    #[test]
    fn edge_bare_sigil_is_reported_not_executed() {
        assert_eq!(
            ReplCommand::parse(":"),
            ReplCommand::Unknown(String::new()),
            "a lone sigil names no command"
        );
        assert_eq!(
            ReplCommand::parse(":"),
            ReplCommand::parse(".   "),
            "either sigil with no word behaves the same"
        );
    }

    /// A command word carries one sigil. Every leading sigil used to be
    /// stripped, so `::load x` and `...quit` reached the command inside them: a
    /// doubled sigil loaded a file and a tripled one ended the session, which is
    /// not what either is.
    #[test]
    fn edge_more_than_one_sigil_names_no_command() {
        for (input, word) in [
            ("::quit", ":quit"),
            ("...quit", "..quit"),
            ("::load some.rb", ":load"),
            ("::vars", ":vars"),
            (".:help", ":help"),
            ("...help", "..help"),
        ] {
            assert_eq!(
                ReplCommand::parse(input),
                ReplCommand::Unknown(word.to_string()),
                "{input:?} is a mistyped command word, and the word left over is \
                 reported rather than run"
            );
        }
    }

    /// Both sigils reach the same command, and nothing about the command depends
    /// on which one the user typed — including its argument.
    #[test]
    fn edge_both_sigils_reach_the_same_command_with_the_same_argument() {
        for name in ReplCommand::bare_names() {
            let colon = ReplCommand::parse(&format!(":{}", name));
            let dot = ReplCommand::parse(&format!(".{}", name));
            assert_eq!(colon, dot, "{name:?} is one command behind either sigil");

            let colon_with = ReplCommand::parse(&format!(":{} example", name));
            let dot_with = ReplCommand::parse(&format!(".{} example", name));
            assert_eq!(
                colon_with, dot_with,
                "{name:?} takes the same argument behind either sigil"
            );
        }
    }

    /// The two lists the completer and `:help` read are the same words: one
    /// without a sigil to complete against, one with the canonical sigil in
    /// front of it to print. A name in one and not the other is the two-table
    /// divergence this table exists to delete.
    #[test]
    fn edge_the_offered_names_are_the_table_s_words_and_nothing_else() {
        let offered = ReplCommand::names();

        assert_eq!(
            offered,
            ReplCommand::bare_names()
                .into_iter()
                .map(|name| format!(":{}", name))
                .collect::<Vec<String>>(),
            "the same words, once each, under the canonical sigil"
        );
        for name in &offered {
            let bare = name.strip_prefix(':').expect("a name carries a sigil");
            assert!(
                !bare.is_empty() && !bare.contains([':', '.']),
                "{name:?} is one word behind one sigil"
            );
        }
    }

    /// Every entry of the table names at least one word.
    ///
    /// `parse` reads `spec.names[0]` to name the command back to the user when
    /// its argument is missing, and `names()`/`bare_names()` only flatten what is
    /// there, so a row added with an empty `names` would be invisible to the
    /// two tests above and would panic the moment somebody typed its sigil.
    #[test]
    fn edge_every_command_in_the_table_names_itself() {
        for spec in super::COMMAND_TABLE {
            assert!(
                !spec.names.is_empty(),
                "{:?} takes an argument but has no word to name it by",
                spec.description
            );
        }
    }

    /// No word is claimed by two commands. The completer's `dedup` would hide a
    /// repeat from a user, and the parser would reach whichever entry came first,
    /// so nothing else would ever say so.
    #[test]
    fn edge_no_command_word_is_claimed_by_two_commands() {
        let names = ReplCommand::bare_names();
        let mut seen: Vec<&String> = Vec::new();

        for name in &names {
            assert!(
                !seen.contains(&name),
                "{name:?} is in the table more than once, so two commands answer to it"
            );
            seen.push(name);
        }
    }
}
