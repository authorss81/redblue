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
    /// Writes the session — every line it ran, what each line said, and what
    /// failed — as Markdown, to `path` or to stdout when given nothing.
    ///
    /// `Option` for the same reason [`ReplCommand::Prompt`] is: `.export` with no
    /// path is not a command missing its argument, it is a command writing to the
    /// terminal, which is the one destination that needs no name.
    Export(Option<String>),
    Reset,
    Run(String),
    /// Opens the debug sub-REPL on an expression or a file, or — with no
    /// argument — on nothing at all, which is the empty session the sub-REPL
    /// commands are then asked against.
    ///
    /// `String` rather than a unit variant for the same reason [`Run`] is: the
    /// argument is the program that runs on entry, and `.debug 1 + 1` that
    /// ignored its argument printed breakpoint status instead of running it.
    /// Same shape as [`ReplCommand::Run`], which does not ignore it either.
    Debug(String),
    /// Registers an expression to be re-evaluated at every step of a program
    /// running inside the debug sub-REPL, reporting it when it changes.
    Watch(String),
    /// Shows the call stack of the last step taken in the debug sub-REPL,
    /// outermost frame first.
    Stack,
    /// Shows the bindings of the frame the debug sub-REPL last stepped in:
    /// its parameters and its own locals, not its caller's.
    Locals,
    /// Leaves the debug sub-REPL, returning the session to its parent prompt
    /// with the parent's state untouched.
    Back,
    /// Sets a breakpoint on a line, or clears one that is already set there.
    Break(String),
    /// Lists the lines the session has breakpoints on.
    Breaks,
    /// Removes the breakpoint on a line.
    Delete(String),
    /// Arms a step into the next call, for the next program the session runs.
    Step,
    /// Arms a step over the next call, for the next program the session runs.
    Next,
    /// Runs the program to its end, or to the next breakpoint.
    Continue,
    /// Prints the frame the program last stopped in.
    Frame,
    Inspect(String),
    /// Answers "what is this?" — the value, its kind, and where it was written.
    ///
    /// The argument is an *expression*, not a name: `.type [1, 2]` is a question
    /// about a value that no session has ever bound.
    Type(String),
    /// Lists what a module exposes, from the registry rather than from prose.
    Doc(String),
    /// Says where a name comes from: a builtin, this session, a module member, or
    /// nothing at all.
    Which(String),
    /// The shape of a call — its parameters and what it answers.
    Sighelp(String),
    /// Runs an expression N times against the session and reports wall time.
    ///
    /// The argument is the measurement, not a name: `.time 1 + 1` is a question
    /// about code the session never bound.
    Time(String),
    /// The same, reported as a distribution over a hundred runs.
    Bench(String),
    Ast(String),
    Tokens(String),
    /// The instruction listing an expression compiles to.
    ///
    /// The argument is an expression, as for [`ReplCommand::Ast`]: the question
    /// is about code, and the code need not be one the session has run.
    Bytecode(String),
    /// The same chunk, annotated: constants table and a comment per instruction.
    Asm(String),
    Example,
    /// The guided tour: a numbered set of steps, each with code to run.
    ///
    /// Separate from [`ReplCommand::Example`] because they answer different
    /// questions — one is "what can I write", the other is "how do I start" — and
    /// a newcomer given the catalogue alone gets a wall rather than a way in.
    Tutorial,
    /// The hint for the tutorial step this session is on.
    ///
    /// A command rather than nothing at all: a reader who has lost the thread of
    /// the tour needs a way back into it that is not retyping it.
    Hint,
    /// Opens `$EDITOR` over the block this session is holding, and runs what
    /// comes back.
    ///
    /// A block can be typed, but a fifty-line one has to be typed: without this
    /// there was no way to *paste* one into a session at all.
    Edit,
    /// Sets the prompt from a format string, or restores the default when given
    /// no argument.
    ///
    /// `Option` rather than a plain `String` because `:prompt` on its own is not
    /// a command missing its argument: it is the one that puts the prompt back.
    /// The table still declares a usage so `:help` names what the argument is.
    Prompt(Option<String>),
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
    /// Whether the argument may be left out without that being an error.
    ///
    /// True for two commands: `:prompt` with nothing after it restores the
    /// default prompt, and `.debug` with nothing after it opens the debug
    /// sub-REPL on nothing yet — a session into which `.watch` and a later
    /// `.debug <file>` are the way in. Each is a thing a user asks for rather
    /// than a command typed incompletely. Both still declare a `usage`, so
    /// `:help` names the format the argument takes.
    optional_argument: bool,
    /// Whether the argument is display text, whose trailing whitespace is part of
    /// it rather than a separator to be dropped.
    ///
    /// True for `:prompt` alone. A prompt ends in a space so what the user types
    /// does not touch it, and every other command's argument is a path or a
    /// program where a trailing space is a typo. So `:prompt rb> ` keeps its
    /// space — which the trimming every other command gets would have taken.
    verbatim_argument: bool,
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

/// The placeholder every command whose argument is a file path declares.
///
/// It is the whole of what says a command takes a path: `:load`, `:save`,
/// `:restore`, `:run` and `.export` declare it, `:break <line>` and `:inspect
/// <name>` declare theirs, and [`ReplCommand::takes_a_file_path`] — which is what
/// decides where TAB offers a path — asks the table rather than keeping a second
/// list of five words to fall out of date.
const FILE_USAGE: &str = "<filename>";

/// Every command, in the order `:help` lists them.
static COMMAND_TABLE: &[CommandSpec] = &[
    CommandSpec {
        names: &["quit", "exit", "q"],
        description: "Exit the REPL",
        usage: None,
        optional_argument: false,
        verbatim_argument: false,
        build: |_| ReplCommand::Quit,
    },
    CommandSpec {
        names: &["help", "h", "?"],
        description: "Show this help",
        usage: None,
        optional_argument: false,
        verbatim_argument: false,
        build: |_| ReplCommand::Help,
    },
    CommandSpec {
        names: &["clear", "cls"],
        description: "Clear the screen",
        usage: None,
        optional_argument: false,
        verbatim_argument: false,
        build: |_| ReplCommand::Clear,
    },
    CommandSpec {
        names: &["history", "hist"],
        description: "Show command history",
        usage: None,
        optional_argument: false,
        verbatim_argument: false,
        build: |_| ReplCommand::History,
    },
    CommandSpec {
        names: &["vars", "variables", "v"],
        description: "Show all variables",
        usage: None,
        optional_argument: false,
        verbatim_argument: false,
        build: |_| ReplCommand::Vars,
    },
    CommandSpec {
        names: &["functions", "funcs", "f"],
        description: "Show defined functions",
        usage: None,
        optional_argument: false,
        verbatim_argument: false,
        build: |_| ReplCommand::Functions,
    },
    CommandSpec {
        names: &["load", "l"],
        description: "Load and run a file",
        usage: Some(FILE_USAGE),
        optional_argument: false,
        verbatim_argument: false,
        build: |argument| ReplCommand::Load(argument.unwrap_or_default()),
    },
    CommandSpec {
        names: &["save", "s"],
        description: "Save current session",
        usage: Some(FILE_USAGE),
        optional_argument: false,
        verbatim_argument: false,
        build: |argument| ReplCommand::Save(argument.unwrap_or_default()),
    },
    CommandSpec {
        names: &["restore", "r"],
        description: "Restore a saved session",
        usage: Some(FILE_USAGE),
        optional_argument: false,
        verbatim_argument: false,
        build: |argument| ReplCommand::Restore(argument.unwrap_or_default()),
    },
    CommandSpec {
        names: &["reset"],
        description: "Reset REPL state",
        usage: None,
        optional_argument: false,
        verbatim_argument: false,
        build: |_| ReplCommand::Reset,
    },
    CommandSpec {
        names: &["export"],
        description: "Write the session as Markdown; no filename writes it here",
        usage: Some(FILE_USAGE),
        optional_argument: true,
        verbatim_argument: false,
        build: |argument| ReplCommand::Export(argument),
    },
    CommandSpec {
        names: &["run"],
        description: "Run a Redblue file",
        usage: Some(FILE_USAGE),
        optional_argument: false,
        verbatim_argument: false,
        build: |argument| ReplCommand::Run(argument.unwrap_or_default()),
    },
    CommandSpec {
        names: &["debug"],
        description: "Open the debug sub-REPL on an expression or a file",
        usage: Some("<expression|file>"),
        optional_argument: true,
        verbatim_argument: false,
        build: |argument| ReplCommand::Debug(argument.unwrap_or_default()),
    },
    CommandSpec {
        names: &["watch"],
        description: "Watch an expression at every step (debug sub-REPL)",
        usage: Some("<expression>"),
        optional_argument: true,
        verbatim_argument: false,
        build: |argument| ReplCommand::Watch(argument.unwrap_or_default()),
    },
    CommandSpec {
        names: &["stack"],
        description: "Show the call stack of the last step (debug sub-REPL)",
        usage: None,
        optional_argument: false,
        verbatim_argument: false,
        build: |_| ReplCommand::Stack,
    },
    CommandSpec {
        names: &["locals"],
        description: "Show the bindings of the frame last stepped in (debug sub-REPL)",
        usage: None,
        optional_argument: false,
        verbatim_argument: false,
        build: |_| ReplCommand::Locals,
    },
    CommandSpec {
        names: &["back"],
        description: "Return to the session from the debug sub-REPL",
        usage: None,
        optional_argument: false,
        verbatim_argument: false,
        build: |_| ReplCommand::Back,
    },
    CommandSpec {
        names: &["break", "b"],
        description: "Stop at a line, or clear the breakpoint there",
        usage: Some("<line>"),
        optional_argument: false,
        verbatim_argument: false,
        build: |argument| ReplCommand::Break(argument.unwrap_or_default()),
    },
    CommandSpec {
        names: &["breaks", "bps"],
        description: "List the lines breakpoints are set on",
        usage: None,
        optional_argument: false,
        verbatim_argument: false,
        build: |_| ReplCommand::Breaks,
    },
    CommandSpec {
        names: &["delete", "d"],
        description: "Remove the breakpoint on a line",
        usage: Some("<line>"),
        optional_argument: false,
        verbatim_argument: false,
        build: |argument| ReplCommand::Delete(argument.unwrap_or_default()),
    },
    CommandSpec {
        names: &["step", "st"],
        description: "Step into the next call",
        usage: None,
        optional_argument: false,
        verbatim_argument: false,
        build: |_| ReplCommand::Step,
    },
    CommandSpec {
        names: &["next", "n"],
        description: "Step over the next call",
        usage: None,
        optional_argument: false,
        verbatim_argument: false,
        build: |_| ReplCommand::Next,
    },
    CommandSpec {
        names: &["continue", "c"],
        description: "Run on to the next breakpoint",
        usage: None,
        optional_argument: false,
        verbatim_argument: false,
        build: |_| ReplCommand::Continue,
    },
    CommandSpec {
        names: &["frame", "fr"],
        description: "Show the frame the program last stopped in",
        usage: None,
        optional_argument: false,
        verbatim_argument: false,
        build: |_| ReplCommand::Frame,
    },
    CommandSpec {
        names: &["inspect", "i"],
        description: "Inspect a variable",
        usage: Some("<variable>"),
        optional_argument: false,
        verbatim_argument: false,
        build: |argument| ReplCommand::Inspect(argument.unwrap_or_default()),
    },
    CommandSpec {
        names: &["type"],
        description: "Value, kind and source span of an expression",
        usage: Some("<expression>"),
        optional_argument: false,
        verbatim_argument: false,
        build: |argument| ReplCommand::Type(argument.unwrap_or_default()),
    },
    CommandSpec {
        names: &["doc"],
        description: "List what a module exposes",
        usage: Some("<module>"),
        optional_argument: false,
        verbatim_argument: false,
        build: |argument| ReplCommand::Doc(argument.unwrap_or_default()),
    },
    CommandSpec {
        names: &["which"],
        description: "Say where a name is bound",
        usage: Some("<name>"),
        optional_argument: false,
        verbatim_argument: false,
        build: |argument| ReplCommand::Which(argument.unwrap_or_default()),
    },
    CommandSpec {
        names: &["sighelp"],
        description: "Show the shape of a call",
        usage: Some("<name>"),
        optional_argument: false,
        verbatim_argument: false,
        build: |argument| ReplCommand::Sighelp(argument.unwrap_or_default()),
    },
    CommandSpec {
        names: &["time"],
        description: "Time an expression over N runs, in wall time",
        usage: Some("<expression> [n times]"),
        optional_argument: false,
        verbatim_argument: false,
        build: |argument| ReplCommand::Time(argument.unwrap_or_default()),
    },
    CommandSpec {
        names: &["bench"],
        description: "Report min/median/max/mean over N runs, in wall time",
        usage: Some("<expression> [n times]"),
        optional_argument: false,
        verbatim_argument: false,
        build: |argument| ReplCommand::Bench(argument.unwrap_or_default()),
    },
    CommandSpec {
        names: &["ast"],
        description: "Print AST for expression",
        usage: Some("<expression>"),
        optional_argument: false,
        verbatim_argument: false,
        build: |argument| ReplCommand::Ast(argument.unwrap_or_default()),
    },
    CommandSpec {
        names: &["tokens"],
        description: "Print tokens for expression",
        usage: Some("<expression>"),
        optional_argument: false,
        verbatim_argument: false,
        build: |argument| ReplCommand::Tokens(argument.unwrap_or_default()),
    },
    CommandSpec {
        names: &["bytecode"],
        description: "List the instructions an expression compiles to",
        usage: Some("<expression>"),
        optional_argument: false,
        verbatim_argument: false,
        build: |argument| ReplCommand::Bytecode(argument.unwrap_or_default()),
    },
    CommandSpec {
        names: &["asm"],
        description: "Show the annotated disassembly of an expression",
        usage: Some("<expression>"),
        optional_argument: false,
        verbatim_argument: false,
        build: |argument| ReplCommand::Asm(argument.unwrap_or_default()),
    },
    CommandSpec {
        names: &["example", "examples"],
        description: "Show example code",
        usage: None,
        optional_argument: false,
        verbatim_argument: false,
        build: |_| ReplCommand::Example,
    },
    CommandSpec {
        names: &["prompt"],
        description: "Set the prompt from {engine}, {version}, {history}; none restores it",
        usage: Some("<format>"),
        optional_argument: true,
        verbatim_argument: true,
        build: |argument| ReplCommand::Prompt(argument),
    },
    CommandSpec {
        names: &["tutorial", "tour"],
        description: "Walk through the guided tour, one numbered step at a time",
        usage: None,
        optional_argument: false,
        verbatim_argument: false,
        build: |_| ReplCommand::Tutorial,
    },
    CommandSpec {
        names: &["hint"],
        description: "Hint for the tutorial step this session is on",
        usage: None,
        optional_argument: false,
        verbatim_argument: false,
        build: |_| ReplCommand::Hint,
    },
    CommandSpec {
        names: &["edit"],
        description: "Edit the block being written in $EDITOR, and run what comes back",
        usage: None,
        optional_argument: false,
        verbatim_argument: false,
        build: |_| ReplCommand::Edit,
    },
];

/// The command word `input` names, with its sigil off.
///
/// Empty when the word is a bare sigil, which is why it is asked for separately
/// rather than inside [`spec_for`]: `:` names no command, and the parser still has
/// to report which word it thought it had.
fn command_word(input: &str) -> &str {
    let input = input.trim_start();
    let word = match input.split_once(char::is_whitespace) {
        Some((word, _)) => word,
        None => input,
    };
    word.strip_prefix(SIGILS).unwrap_or(word)
}

/// The table entry a line names, or `None` when it names none.
///
/// The lookup [`ReplCommand::parse`] and [`ReplCommand::keeps_argument_whitespace`]
/// both need, so it lives here rather than being written twice: a second copy of
/// the "which sigil, which word, which entry" question is a command that parses
/// and is invisible to whoever asks about it.
fn spec_for(input: &str) -> Option<&'static CommandSpec> {
    let command = command_word(input);
    COMMAND_TABLE
        .iter()
        .find(|spec| spec.names.contains(&command))
}

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
        // Whitespace and nothing else is no argument at all, for the one command
        // whose argument keeps its trailing spaces: `:prompt   ` is `:prompt`,
        // not a prompt made of three spaces.
        if argument.trim().is_empty() {
            None
        } else {
            Some(argument.to_string())
        }
    }

    /// A path with one matching pair of quotes around it, and without them.
    ///
    /// TAB quotes a filename that holds a space — see
    /// [`super::completer::complete_path_in`] — so this is the other half of that:
    /// without it the completer offers `"my file.rb"`, accepting it puts those quotes
    /// in the command, and the command then tries to open a file whose name really
    /// does start and end with a `"` character.
    ///
    /// Only a *matching* pair, only around the whole argument, and only for the
    /// commands whose argument is a path. `:prompt "rb> "` is display text and its
    /// quotes are part of it, which is the whole reason this is not in
    /// [`ReplCommand::argument`].
    fn unquote_path(path: &str) -> String {
        let quoted = path.len() >= 2 && path.starts_with('"') && path.ends_with('"');
        if quoted {
            path[1..path.len() - 1].to_string()
        } else {
            path.to_string()
        }
    }

    pub fn parse(input: &str) -> Self {
        let input = input.trim_start();

        if !input.starts_with(SIGILS) {
            return ReplCommand::NotACommand;
        }

        match spec_for(input) {
            Some(spec) => match spec.usage {
                None => (spec.build)(None),
                Some(usage) => match Self::argument(if spec.verbatim_argument {
                    input
                } else {
                    input.trim_end()
                }) {
                    Some(value) => {
                        let value = if spec.usage == Some(FILE_USAGE) {
                            Self::unquote_path(&value)
                        } else {
                            value
                        };
                        (spec.build)(Some(value))
                    }
                    // An argument the command does not insist on is the command's
                    // own business, not a missing-argument report: `:prompt` reads
                    // "no format given" as "put the default back".
                    None if spec.optional_argument => (spec.build)(None),
                    None => ReplCommand::MissingArgument {
                        command: spec.names[0],
                        usage,
                    },
                },
            },
            None => ReplCommand::Unknown(command_word(input).to_string()),
        }
    }

    /// Whether the argument this command carries is a file path.
    ///
    /// Asked by the completer before it offers the filesystem: TAB completes a
    /// path in the argument of a command that takes one, and nowhere else, so
    /// `say "` is not answered with a directory listing.
    ///
    /// Read out of the table by its `<filename>` placeholder rather than kept as a
    /// list here, for the reason the whole table exists: a second copy of the
    /// answer is a second answer.
    ///
    /// A sigil is required, and by the same first step [`ReplCommand::parse`]
    /// takes: `load x` with no sigil is not a command at all, it is Redblue source
    /// calling a function called `load`, and offering it a filesystem would be
    /// offering paths to a line that cannot take one.
    pub fn takes_a_file_path(input: &str) -> bool {
        let input = input.trim_start();
        input.starts_with(SIGILS)
            && spec_for(input).is_some_and(|spec| spec.usage == Some(FILE_USAGE))
    }

    /// Whether `input` names a command whose argument keeps the whitespace at its
    /// end.
    ///
    /// Asked by the session before it records a line. `:prompt rb> ` is display
    /// text: the space at the end of it is the space between the prompt and what
    /// the user types, so a history entry that dropped it would come back as a
    /// different prompt. Every other command's argument is a path or a program,
    /// where a trailing space is a typo, and those are recorded trimmed.
    pub fn keeps_argument_whitespace(input: &str) -> bool {
        spec_for(input).is_some_and(|spec| spec.verbatim_argument)
    }

    /// The argument this command carries, or `None` for one that takes none.
    ///
    /// Read by the tests rather than by the session, which dispatches on the
    /// variant: an alias table that asserted only the variant could not tell
    /// `:ast 1 + 1` carrying `Ast("1")` from carrying `Ast("1 + 1")`, which is
    /// the bug that table exists to keep fixed.
    pub fn argument_text(&self) -> Option<&str> {
        match self {
            ReplCommand::Load(value)
            | ReplCommand::Save(value)
            | ReplCommand::Restore(value)
            | ReplCommand::Run(value)
            | ReplCommand::Inspect(value)
            | ReplCommand::Type(value)
            | ReplCommand::Doc(value)
            | ReplCommand::Which(value)
            | ReplCommand::Sighelp(value)
            | ReplCommand::Time(value)
            | ReplCommand::Bench(value)
            | ReplCommand::Ast(value)
            | ReplCommand::Tokens(value)
            | ReplCommand::Bytecode(value)
            | ReplCommand::Asm(value)
            | ReplCommand::Break(value)
            | ReplCommand::Delete(value)
            | ReplCommand::Debug(value)
            | ReplCommand::Watch(value)
            | ReplCommand::Export(Some(value)) => Some(value.as_str()),
            ReplCommand::Prompt(Some(value)) => Some(value.as_str()),
            _ => None,
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
            // A quoted path is the form TAB offers for a name holding a space, so
            // it is the form the command has to open. `:prompt "rb> "` is not a
            // path and keeps its quotes: they are part of what a prompt is.
            (":load \"my file.rb\"", "Load", "my file.rb"),
            (":prompt \"rb> \"", "Prompt", "\"rb> \""),
            (":save out.txt", "Save", "out.txt"),
            (":s out.txt", "Save", "out.txt"),
            (":restore out.txt", "Restore", "out.txt"),
            (":r out.txt", "Restore", "out.txt"),
            // `.export` is the one command that takes a path and may be given
            // none: with no argument it writes the session to the terminal, which
            // is a thing a user asks for rather than a command typed
            // incompletely. Both shapes are here, and the variant tells them
            // apart — `Export(None)` is the document on stdout and
            // `Export(Some(..))` is one written to a file.
            (":export", "Export", ""),
            (":export   ", "Export", ""),
            (":export session.md", "Export", "session.md"),
            (":reset", "Reset", ""),
            (":run prog.rb", "Run", "prog.rb"),
            (":debug", "Debug", ""),
            (".debug prog.rb", "Debug", "prog.rb"),
            (".watch x", "Watch", "x"),
            (".stack", "Stack", ""),
            (".locals", "Locals", ""),
            (".back", "Back", ""),
            (":break 3", "Break", "3"),
            (":b 3", "Break", "3"),
            (":breaks", "Breaks", ""),
            (":bps", "Breaks", ""),
            (":delete 3", "Delete", "3"),
            (":d 3", "Delete", "3"),
            (":step", "Step", ""),
            (":st", "Step", ""),
            (":next", "Next", ""),
            (":n", "Next", ""),
            (":continue", "Continue", ""),
            (":c", "Continue", ""),
            (":frame", "Frame", ""),
            (":fr", "Frame", ""),
            (":inspect x", "Inspect", "x"),
            (":i x", "Inspect", "x"),
            (":ast 1 + 1", "Ast", "1 + 1"),
            (":tokens 1 + 1", "Tokens", "1 + 1"),
            (":example", "Example", ""),
            (":examples", "Example", ""),
            // `:prompt` is the one command whose argument may be left out and the
            // one whose argument keeps its trailing whitespace, so it is listed in
            // all three of its shapes: bare, whitespace only, and with the space
            // a prompt ends with. Nothing here says which shape is which — the
            // rows are read back as the exact variant, so a `Prompt(Some("rb>"))`
            // where `Prompt(Some("rb> "))` was asked for fails.
            (":prompt", "Prompt", ""),
            (":prompt   ", "Prompt", ""),
            (":prompt rb> ", "Prompt", "rb> "),
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
                | ReplCommand::Tokens(value)
                | ReplCommand::Break(value)
                | ReplCommand::Debug(value)
                | ReplCommand::Watch(value)
                | ReplCommand::Delete(value) => value.as_str(),
                // The two `Prompt` shapes are told apart by the variant itself:
                // `Prompt(None)` is a command that restores the default and falls
                // to the empty argument below, `Prompt(Some(..))` is one that was
                // given a format and carries it verbatim.
                ReplCommand::Prompt(Some(value)) => value.as_str(),
                // `.export` with a path carries it the same way; with none it
                // carries nothing, which is the empty argument above.
                ReplCommand::Export(Some(value)) => value.as_str(),
                _ => "",
            };
            assert_eq!(
                carried, argument,
                "parsing {:?} should carry the argument {:?}",
                input, argument
            );
        }
    }

    /// The four questions the REPL could not answer about code, as commands.
    ///
    /// Before this there was no word in the table that inspected anything: the
    /// seventeen commands were all about the *session*, so "what is `uppercase`"
    /// or "what kind of thing is `[1, 2]`" meant leaving the REPL. Asserted
    /// through [`ReplCommand::parse`] rather than against [`COMMAND_TABLE`]
    /// directly, because what a user types reaching a command is the thing that
    /// was missing.
    #[test]
    fn parse_answers_questions_about_code() {
        let cases: Vec<(&str, &str, &str)> = vec![
            (":type [1, 2]", "Type", "[1, 2]"),
            (".type nosuchname", "Type", "nosuchname"),
            (":doc json", "Doc", "json"),
            (":doc nosuchmodule", "Doc", "nosuchmodule"),
            (":which uppercase", "Which", "uppercase"),
            (".which nosuchname", "Which", "nosuchname"),
            (":sighelp split", "Sighelp", "split"),
            (":sighelp say", "Sighelp", "say"),
        ];

        for (input, expected, argument) in cases {
            let parsed = ReplCommand::parse(input);
            let variant = format!("{:?}", parsed);
            let variant = variant.split(['(', ' ']).next().unwrap_or_default();
            assert_eq!(
                variant, expected,
                "{:?} is a command a question about code needs",
                input
            );
            assert_eq!(
                parsed.argument_text(),
                Some(argument),
                "{:?} carries what was asked about",
                input
            );
        }
    }

    /// `:prompt` is the only command whose argument may be left out and the only
    /// one whose argument keeps the whitespace at its end, so both of those flags
    /// in the table are read here on all three shapes.
    ///
    /// A prompt ends in a space — that space is between the prompt and what the
    /// user types — so `:prompt rb> ` and `:prompt rb>` are different prompts. And
    /// `:prompt` on its own is not a command typed incompletely: it is the one
    /// that puts the default back. Both facts are only worth anything if the
    /// parser keeps them, which is what this asserts directly rather than through
    /// the alias table, where a `Prompt` row could pass while the argument it
    /// carries was wrong.
    #[test]
    fn parse_reads_a_prompt_as_display_text_or_as_no_argument() {
        assert_eq!(
            ReplCommand::parse(":prompt"),
            ReplCommand::Prompt(None),
            "no argument at all is the command that restores the default"
        );
        assert_eq!(
            ReplCommand::parse(":prompt   "),
            ReplCommand::Prompt(None),
            "and whitespace is not a prompt made of three spaces"
        );
        assert_eq!(
            ReplCommand::parse(":prompt rb> "),
            ReplCommand::Prompt(Some("rb> ".to_string())),
            "the space a prompt ends with is part of the prompt"
        );
        assert_eq!(
            ReplCommand::parse("  :prompt rb>  "),
            ReplCommand::Prompt(Some("rb>  ".to_string())),
            "leading whitespace is the separator; every trailing space is the prompt's"
        );
        assert_eq!(
            ReplCommand::parse(".prompt rb> "),
            ReplCommand::Prompt(Some("rb> ".to_string())),
            "and the other sigil reaches it with the same argument"
        );
        assert_eq!(
            ReplCommand::parse(":prompt {version} "),
            ReplCommand::Prompt(Some("{version} ".to_string())),
            "a placeholder is not substituted here — the session renders it"
        );
        // The other end of the rule: a command whose argument is not display text
        // still has its trailing whitespace dropped, so the two flags are not one
        // flag.
        assert_eq!(
            ReplCommand::parse(":inspect counter  "),
            ReplCommand::Inspect("counter".to_string()),
            "a variable name does not keep the spaces typed after it"
        );
        assert_eq!(
            ReplCommand::parse(":load"),
            ReplCommand::MissingArgument {
                command: "load",
                usage: "<filename>"
            },
            "and an optional argument is not a licence for every command to have one"
        );
    }

    /// The debug sub-REPL's commands, as the words a user types them.
    ///
    /// The sub-REPL exists to answer `.debug <expr|file>` — `.watch`, `.stack`,
    /// `.locals` and `.back` are the questions inside it, and none of them
    /// reached a command before this: `.watch x` was the unknown word `watch`,
    /// so the session that needed a watch had no way to set one. Asserted
    /// through [`ReplCommand::parse`] like the alias table, because a word a
    /// user types reaching nothing is the whole of the failure.
    ///
    /// `.debug` is listed in both of its shapes: bare, which opens the sub-REPL
    /// with nothing run yet, and with the argument that runs on entry. The
    /// argument is checked as text rather than as a variant so this test is the
    /// same test before and after the sub-REPL exists.
    #[test]
    fn the_debug_sub_repl_commands_parse() {
        let cases: Vec<(&str, &str, &str)> = vec![
            (".debug", "Debug", ""),
            (":debug", "Debug", ""),
            (".debug prog.rb", "Debug", "prog.rb"),
            (".debug 1 + 1", "Debug", "1 + 1"),
            (".watch x", "Watch", "x"),
            (":watch count + 1", "Watch", "count + 1"),
            (".stack", "Stack", ""),
            (".locals", "Locals", ""),
            (".back", "Back", ""),
            (":back", "Back", ""),
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
            let carried = parsed.argument_text().unwrap_or_default();
            assert_eq!(
                carried, argument,
                "parsing {:?} should carry the argument {:?}",
                input, argument
            );
        }
    }

    /// Whether a line has to be recorded the way it was typed, which is a
    /// question about the table and not about the line's contents.
    ///
    /// The session asks this before it puts a line in the history, and gets it
    /// wrong in the direction that loses text: recording `:prompt rb> ` trimmed
    /// would make `!!` repeat a prompt with no space in it, and recording `2 + 3
    /// ` verbatim would put a trailing space in the history file for no reason.
    #[test]
    fn a_line_is_recorded_verbatim_only_for_a_display_text_argument() {
        assert!(
            ReplCommand::keeps_argument_whitespace(":prompt rb> "),
            "a prompt's trailing space is the prompt"
        );
        assert!(
            ReplCommand::keeps_argument_whitespace(":prompt"),
            "and so is the decision about which prompt, whether or not there is one"
        );
        for line in [
            "2 + 3  ",
            ":load /tmp/a.rb  ",
            ":inspect counter  ",
            "",
            "   ",
            ":bogus argument  ",
            "::prompt rb> ",
        ] {
            assert!(
                !ReplCommand::keeps_argument_whitespace(line),
                "{line:?} is not a display-text command, so it is recorded trimmed"
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
            ReplCommand::parse(".bytecode say 1 + 1"),
            ReplCommand::Bytecode("say 1 + 1".to_string()),
            "the compiler questions take the whole expression, on either sigil"
        );
        assert_eq!(
            ReplCommand::parse(".asm set x to 1"),
            ReplCommand::Asm("set x to 1".to_string())
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
    /// `.export` is a command, and it is the one whose argument may be left out.
    ///
    /// There was no way to write a session down at all: `.export` was reported as
    /// the unknown word `export`, so sharing a session meant copy-pasting terminal
    /// scrollback — prompts, colour and banner included — which is not a thing
    /// that can be pasted into an issue and read.
    ///
    /// Both shapes matter and the variant is what tells them apart: `.export` with
    /// a path writes a file, `.export` with none writes to the terminal. A command
    /// that reported the second as a missing argument would push every user to a
    /// temporary file for the one case that needs none.
    #[test]
    fn edge_export_is_a_command_whose_path_may_be_left_out() {
        assert_eq!(
            ReplCommand::parse(".export session.md"),
            ReplCommand::Export(Some("session.md".to_string())),
            "`.export` with a path is a document written to that path"
        );
        assert_eq!(
            ReplCommand::parse(":export"),
            ReplCommand::Export(None),
            "and with no path it is a document written to the terminal, which is \
             why the argument may be left out"
        );
        assert_eq!(
            ReplCommand::parse(".export   "),
            ReplCommand::Export(None),
            "whitespace after the command word is not a path"
        );
        assert!(
            ReplCommand::names().contains(&":export".to_string()),
            "and it is offered by the completer, which offers the parser's table"
        );
    }

    /// The commands whose argument is a path, and only those.
    ///
    /// This is the gate on where TAB offers the filesystem. It is read out of the
    /// table by its filename placeholder, so a sixth command that takes a path
    /// says so by declaring the same thing — and a command that takes something
    /// else cannot be completed as though it took a path.
    #[test]
    fn edge_only_the_commands_that_take_a_path_take_a_path() {
        let taking: Vec<String> = ReplCommand::bare_names()
            .into_iter()
            .filter(|name| ReplCommand::takes_a_file_path(&format!(":{name}")))
            .collect();

        assert_eq!(
            taking,
            vec![
                "export".to_string(),
                "l".to_string(),
                "load".to_string(),
                "r".to_string(),
                "restore".to_string(),
                "run".to_string(),
                "s".to_string(),
                "save".to_string(),
            ],
            "the five commands that take a filename, aliases included, and \
             nothing else: a command taking a line number or an expression is not \
             one whose argument can be completed as a path"
        );

        for (command, takes) in [
            (":load x", true),
            (".save x", true),
            (":run x", true),
            (":restore x", true),
            (":export x", true),
            (":break 3", false),
            (":inspect x", false),
            (":ast 1 + 1", false),
            (":prompt rb> ", false),
            ("say \"x", false),
            ("", false),
        ] {
            assert_eq!(
                ReplCommand::takes_a_file_path(command),
                takes,
                "{command:?} is or is not a command whose argument is a path"
            );
        }
    }
}
