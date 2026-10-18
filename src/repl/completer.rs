use std::path::{Path, PathBuf};

use super::commands;

/// The separator offered after a directory, so it is visibly a directory and the
/// next TAB walks into it.
///
/// `/` rather than [`std::path::MAIN_SEPARATOR`]: this is the text that goes into
/// a command line, and `:load src/main.rs` reads the same on every platform.
/// Both separators are *accepted* when one is being completed (see
/// [`split_fragment`]), so offering this one on Windows is cosmetic rather than
/// wrong.
const DIRECTORY_SEPARATOR: &str = "/";

/// How many entries one directory offers before the list is cut short.
///
/// A completion list is read by eye, and a directory of ten thousand entries is
/// not a list any reader can use: it is the answer to "did I mean this
/// directory?", not a catalogue. The list simply stops at this many, which reads
/// as "type more to see less" — an entry saying how many were left out would be
/// one more thing in a column of filenames, and accepting it would type a name
/// that is not there.
pub const MAX_PATH_CANDIDATES: usize = 200;

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

#[derive(Clone)]
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
            // The module names are the language's table, read rather than
            // written out again — the same reason `commands` below is
            // `ReplCommand`'s own list. The hand-kept copy had drifted both ways:
            // it offered `formats`, which no program can name, and did not offer
            // five modules that answer — `bytes`, `csv`, `json`, `sys`, `time`.
            builtins: module_names()
                .into_iter()
                .chain(vec![
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
                    "random_seed".to_string(),
                    "uppercase".to_string(),
                    "lowercase".to_string(),
                    "trim".to_string(),
                    "split".to_string(),
                    "join".to_string(),
                    "contains".to_string(),
                ])
                .collect(),
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

/// Where the cursor is, when it is in the path argument of a command that takes
/// one: the offset that argument starts at, and the fragment typed so far.
///
/// `None` for everything else — and that is the property this exists for. A
/// completer that offered paths anywhere would answer `say "` with a directory
/// listing, so the question is asked of the *command* first and answered by
/// [`commands::ReplCommand::takes_a_file_path`], which is the parser's own table
/// rather than a list of the commands that happen to take a path today. The two
/// path commands of the file loop, `.reload` and `.format`, are offered paths
/// because they declare the same `<filename>` placeholder. Nothing else is.
///
/// A cursor the caller put inside a character is `None` rather than a panic:
/// `line.get(..pos)` is what says so.
pub fn path_argument(line: &str, pos: usize) -> Option<(usize, &str)> {
    let head = line.get(..pos)?;
    let command_end = head.find(char::is_whitespace)?;
    if !commands::ReplCommand::takes_a_file_path(&head[..command_end]) {
        return None;
    }
    let rest = &head[command_end..];
    let start = command_end + rest.len() - rest.trim_start().len();
    Some((start, &head[start..]))
}

/// The directory part of `fragment` and the name being typed in it, as the user
/// wrote them.
///
/// Both halves are text rather than a [`Path`], because the completion is offered
/// back to be typed: `:load ./sr` is completed to `./src/lib.rs`, not to the
/// normalised `src/lib.rs` the filesystem would answer to. Both `/` and the
/// platform's own separator end the directory, so a path typed on either is
/// walked into.
fn split_fragment(fragment: &str) -> (&str, &str) {
    let separator = |c: char| c == '/' || (std::path::is_separator(c) && c != '/');
    match fragment.rfind(separator) {
        Some(at) => fragment.split_at(at + 1),
        None => ("", fragment),
    }
}

/// Completes `fragment` against what is in `dir`, offering directories with a
/// trailing separator.
///
/// `dir` is named rather than read from the process, so a caller can complete
/// against a directory it chose and a test can complete against a scratch one
/// instead of whatever the checkout happens to hold.
///
/// Three things about the answer, each of them a decision rather than an
/// accident:
///
/// - **Directories are offered with `/` after them.** A completion that reads
///   `src` and `src.rs` as two answers leaves the reader to guess which is a
///   directory; a trailing separator says so, and lets the next TAB walk in.
/// - **A name with a space in it is quoted.** The argument is everything after
///   the command word, so an unquoted `my file.rb` would reach the command as
///   two arguments — one path and one stray word.
/// - **Dot-entries are not offered.** A directory listing a user did not ask for
///   is noise, and `.git` in it is noise twice over; a hidden file is still
///   reachable by typing its name.
///
/// A directory that cannot be read — missing, unreadable, not a directory, a
/// name that is not valid UTF-8 on this platform — offers nothing rather than
/// failing: a TAB that reports nothing is better than a TAB that prints.
pub fn complete_path_in(dir: &Path, fragment: &str) -> Vec<String> {
    let is_quoted = fragment.starts_with('"');
    let unquoted = fragment.trim_matches('"');
    let (directory, name) = split_fragment(unquoted);
    let listing = dir.join(directory);
    let prefix = name.to_lowercase();
    let Ok(entries) = std::fs::read_dir(&listing) else {
        return Vec::new();
    };

    let mut candidates: Vec<String> = Vec::new();
    for entry in entries.flatten() {
        let Some(entry_name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if entry_name.starts_with('.') || !entry_name.to_lowercase().starts_with(&prefix) {
            continue;
        }
        let sep = if is_directory(&entry.path()) {
            DIRECTORY_SEPARATOR
        } else {
            ""
        };
        let joined = format!("{directory}{entry_name}{sep}");
        let candidate = if is_quoted || joined.contains(char::is_whitespace) {
            format!("\"{joined}\"")
        } else {
            joined
        };
        candidates.push(candidate);
    }

    // Sorted before it is cut short, because a filesystem does not hand entries
    // back in an order this can promise: a list that depended on readdir would be
    // a different list on a different day, and a test could not pin it. That is
    // also why the directory is read whole and the *list* is capped — a list
    // sorted from a partial reading is a list of whichever entries the filesystem
    // returned first, which is not a promise anyone can keep.
    candidates.sort_by_key(|candidate| sort_key(candidate));
    candidates.truncate(MAX_PATH_CANDIDATES);
    candidates
}

/// The case-insensitive order a reader expects of a file list, with the exact
/// spelling as the tiebreak so two names differing only in case have one order.
fn sort_key(candidate: &str) -> (String, String) {
    (candidate.to_lowercase(), candidate.to_string())
}

/// Whether `path` is a directory, following a symlink to one.
///
/// `metadata` rather than `file_type`, which reports the link itself: a symlink
/// into a directory is a thing you can walk into, and offering it without the
/// separator would send the next TAB to the parent instead. A path whose type
/// cannot be read is offered as a file, which is the reading that does least.
fn is_directory(path: &Path) -> bool {
    std::fs::metadata(path)
        .map(|metadata| metadata.is_dir())
        .unwrap_or(false)
}

impl ReplCompleter {
    /// What TAB offers when the cursor is in the path argument of a command that
    /// takes one, and `None` when it is anywhere else.
    ///
    /// The completion rather than the candidates alone, because the offset is half
    /// of the answer: rustyline replaces from there, so a candidate that did not
    /// come with the place it starts would replace the command word too.
    pub fn complete_path(&self, line: &str, pos: usize) -> Option<(usize, Vec<String>)> {
        let (start, fragment) = path_argument(line, pos)?;
        Some((start, complete_path_in(&current_dir(), fragment)))
    }
}

/// The directory a session completes paths against.
///
/// A function so that the answer is one place: the process's working directory,
/// which is where a user typing `:load ` means "here".
fn current_dir() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::{complete_path_in, path_argument, ReplCompleter, MAX_PATH_CANDIDATES};
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

    /// Every builtin the completer offers is a name a program can actually name:
    /// a module, a registered builtin or a global constant.
    ///
    /// Tab-completion is the one place the language *invites* a name rather than
    /// waiting to be asked, so a name here is a promise — and this table held six
    /// it could not keep. `formats` is the one this phase is about (SPEC.md and
    /// README.md both stopped naming it in rounds 1 and 2; the completer still
    /// offered it), and `parse_json`, `to_json`, `read`, `write` and `exists` are
    /// the same fiction flat-spelled: each answers `Unknown function`, because
    /// JSON is `json.parse` and the file functions are `files.read`.
    #[test]
    fn edge_the_repl_offers_no_builtin_a_program_cannot_call() {
        let completer = ReplCompleter::new();

        for name in &completer.builtins {
            assert!(
                crate::stdlib::is_module(name)
                    || crate::stdlib::is_builtin(name)
                    || crate::stdlib::is_global(name),
                "{name} is offered as a builtin, so it has to be one a program can name: \
                 it is not a module, not a registered builtin, and not a global"
            );
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
    /// A scratch directory for the path tests, named so no other test in this
    /// binary can be looking at it, and emptied before use so what a test finds
    /// does not depend on what ran before it.
    fn scratch_dir(name: &str) -> PathBuf {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let serial = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/tmp/repl-completer")
            .join(format!("{name}-{serial}"));
        if dir.exists() {
            fs::remove_dir_all(&dir).expect("a leftover scratch directory should be removable");
        }
        fs::create_dir_all(&dir).expect("scratch directory should be creatable");
        dir
    }

    /// A file in `dir`, written empty so it exists to be listed.
    fn file_in(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, "").expect("the scratch file should be writable");
        path
    }

    fn dir_in(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        fs::create_dir_all(&path).expect("the scratch directory should be creatable");
        path
    }

    /// The finding, both halves of it: the file commands complete paths, and
    /// every other word completes vocabulary.
    ///
    /// `:load sr` is the case that matters. It used to be completed against the
    /// command table and the keyword table — `:save`, `say`, `skip`, `split` —
    /// so the one place a user has to name a file on disk was the one place with
    /// no knowledge of the disk.
    #[test]
    fn a_file_command_completes_the_paths_it_could_load() {
        let dir = scratch_dir("load");
        dir_in(&dir, "src");
        file_in(&dir, "notes.txt");

        assert_eq!(
            complete_path_in(&dir, "sr"),
            vec!["src/".to_string()],
            "a directory is offered with the separator that says it is one"
        );
        assert_eq!(
            complete_path_in(&dir, "no"),
            vec!["notes.txt".to_string()],
            "a file is offered whole"
        );
        assert_eq!(
            complete_path_in(&dir, "src/"),
            Vec::<String>::new(),
            "a directory with nothing in it offers nothing of its contents"
        );
        assert_eq!(
            complete_path_in(&dir, "src/n"),
            Vec::<String>::new(),
            "and a path walked into is completed against what is really there"
        );
        assert_eq!(
            complete_path_in(&dir, "zzz"),
            Vec::<String>::new(),
            "a prefix nothing starts with completes to nothing, as everywhere else"
        );
    }

    /// An empty fragment offers the directory the user is in, and the cursor's
    /// offset is the whole argument — so `:load ` followed by TAB replaces the
    /// empty argument and leaves the command word alone.
    #[test]
    fn an_empty_fragment_offers_the_directory_and_replaces_only_the_argument() {
        let dir = scratch_dir("empty-fragment");
        file_in(&dir, "a.rb");
        file_in(&dir, "b.md");

        assert_eq!(
            complete_path_in(&dir, ""),
            vec!["a.rb".to_string(), "b.md".to_string()],
            "an empty fragment is every entry, in order"
        );

        assert_eq!(
            path_argument(":load ", 6),
            Some((6, "")),
            "the argument starts after the command word and its space"
        );
        assert_eq!(
            path_argument(":load src/li", 12),
            Some((6, "src/li")),
            "and the fragment is everything from there to the cursor, so the \\
             completion replaces the path and not the command"
        );
        assert_eq!(
            path_argument(".save out.txt", 13),
            Some((6, "out.txt")),
            "the second sigil is the same command and completes the same way"
        );
    }

    /// A name with a space in it is offered quoted, so that accepting it does not
    /// hand the command two arguments.
    ///
    /// `ReplCommand::argument` is everything after the command word, so an
    /// unquoted `my file.rb` reaches `:load` as `my file.rb` — which happens to
    /// be the whole of it — but a quoted one reaches it *with* the quotes, and
    /// neither is a path that opens. Quoting is the form that says which one is
    /// the name.
    #[test]
    fn edge_a_filename_with_a_space_in_it_is_offered_quoted() {
        let dir = scratch_dir("spaced");
        file_in(&dir, "my file.rb");
        file_in(&dir, "plain.rb");
        let sub = dir.join("my dir");
        std::fs::create_dir_all(&sub).expect("create dir");
        file_in(&sub, "nested.rb");

        let offered = complete_path_in(&dir, "my");

        assert_eq!(
            offered,
            vec!["\"my dir/\"".to_string(), "\"my file.rb\"".to_string()],
            "quoted, because an unquoted name with a space in it is two words"
        );
        assert_eq!(
            complete_path_in(&dir, "\"my dir/"),
            vec!["\"my dir/nested.rb\"".to_string()],
            "and walking into a quoted directory completes entries inside it"
        );
        assert_eq!(
            complete_path_in(&dir, "\"my dir/\""),
            vec!["\"my dir/nested.rb\"".to_string()],
            "whether closed with a quote or left open"
        );
        assert_eq!(
            complete_path_in(&dir, "pl"),
            vec!["plain.rb".to_string()],
            "and a name nobody needed quoting is not given any"
        );
    }

    /// The three shapes of nothing, all of which a TAB can meet and none of which
    /// may be an error: a directory with nothing in it, a prefix nothing starts
    /// with, and a path into a directory that is not there.
    #[test]
    fn edge_nothing_to_offer_is_no_candidates_rather_than_a_failure() {
        let empty = scratch_dir("empty");

        assert!(
            complete_path_in(&empty, "").is_empty(),
            "an empty directory has nothing to offer, which is not a failure"
        );
        assert!(
            complete_path_in(&empty, "anything").is_empty(),
            "and nothing there starts with the prefix either"
        );
        assert!(
            complete_path_in(&empty.join("no-such-directory"), "x").is_empty(),
            "a directory that cannot be read offers nothing rather than panicking"
        );
        assert!(
            complete_path_in(&file_in(&empty, "a-file"), "x").is_empty(),
            "and a path that is a file rather than a directory is the same answer"
        );
        assert_eq!(
            path_argument(":load ", 1000),
            None,
            "a cursor past the end of the line is not a position to complete at"
        );
        assert_eq!(
            path_argument(":load \u{1F600}", 7),
            None,
            "and a cursor inside a character is not one either"
        );
        assert_eq!(path_argument(":load \u{1F600}", 8), None);
        assert_eq!(path_argument(":load \u{1F600}", 9), None);
    }

    /// A dot-entry is not offered, which is what keeps `.git` and `.env` out of a
    /// listing — and it is a decision rather than an accident, so it is pinned:
    /// the hidden file is still reachable by typing it.
    #[test]
    fn edge_a_dot_entry_is_not_offered() {
        let dir = scratch_dir("hidden");
        file_in(&dir, ".hidden");
        file_in(&dir, "shown.rb");

        assert_eq!(
            complete_path_in(&dir, ""),
            vec!["shown.rb".to_string()],
            "a dot-entry is a thing the user did not mean to name"
        );
        assert_eq!(
            complete_path_in(&dir, ".hid"),
            Vec::<String>::new(),
            "and it is not reachable by prefixing it either"
        );
        assert!(
            dir.join(".hidden").exists(),
            "which is only about what is offered: the file is still there to be \\
             typed, and nothing here touched it"
        );
    }

    /// A long list is cut short, and it is cut in an order the filesystem does not
    /// choose — a second run over the same directory answers the same way.
    #[test]
    fn edge_a_long_listing_is_capped_and_the_order_does_not_depend_on_the_disk() {
        let dir = scratch_dir("capped");
        for index in 0..MAX_PATH_CANDIDATES + 10 {
            file_in(&dir, &format!("{index:04}.rb"));
        }

        let offered = complete_path_in(&dir, "");

        assert_eq!(
            offered.len(),
            MAX_PATH_CANDIDATES,
            "a list longer than a reader can use is cut short"
        );
        assert_eq!(
            offered,
            complete_path_in(&dir, ""),
            "and the same directory answers the same way twice: the list is \\
             sorted before it is cut, because readdir does not promise an order"
        );
        assert_eq!(
            offered.first().map(String::as_str),
            Some("0000.rb"),
            "so the cut is the first of the sorted list, not whichever entries the \\
             filesystem happened to return first"
        );
    }

    /// A symlink to a directory is walked into, because it is a thing you can walk
    /// into — and offering it as a file would send the next TAB to its parent.
    #[test]
    fn a_symlink_to_a_directory_is_offered_as_a_directory() {
        let dir = scratch_dir("symlink");
        dir_in(&dir, "real");
        #[cfg(unix)]
        std::os::unix::fs::symlink(dir.join("real"), dir.join("link")).expect("a symlink");

        let offered = complete_path_in(&dir, "l");

        #[cfg(unix)]
        assert_eq!(
            offered,
            vec!["link/".to_string()],
            "the link is followed, so it is offered with the separator"
        );
        #[cfg(not(unix))]
        assert!(
            offered.is_empty(),
            "no symlink was made, so nothing is offered"
        );
    }

    /// The filesystem is read for a path argument and nowhere else, at every
    /// position in the line.
    #[test]
    fn a_path_is_offered_only_in_the_argument_of_a_command_that_takes_one() {
        for (line, pos, expected) in [
            (":load s", 7, Some(6)),
            (".save sr", 7, Some(6)),
            (":run s", 6, Some(5)),
            (":restore s", 10, Some(9)),
            (":l s", 4, Some(3)),
            (":load", 5, None),
            (":loadx s", 8, None),
            ("::load s", 8, None),
            ("load s", 6, None),
            ("say s", 5, None),
            ("say \"s", 6, None),
            ("", 0, None),
        ] {
            assert_eq!(
                path_argument(line, pos).map(|(start, _)| start),
                expected,
                "{line:?} at {pos} is {} a path argument",
                if expected.is_some() { "in" } else { "not in" }
            );
        }
    }
}
