use std::collections::VecDeque;
use std::path::Path;

pub struct ReplHistory {
    history: VecDeque<String>,
    max_size: usize,
}

impl ReplHistory {
    pub fn new(max_size: usize) -> Self {
        Self {
            history: VecDeque::with_capacity(max_size),
            max_size,
        }
    }

    pub fn push(&mut self, entry: String) {
        if self.history.back() == Some(&entry) {
            return;
        }

        // A limit of zero means "keep nothing". Without this the eviction below
        // pops from an already-empty history and the push puts one entry back,
        // so a zero-sized history holds one entry.
        if self.max_size == 0 {
            return;
        }

        if self.history.len() >= self.max_size {
            self.history.pop_front();
        }

        self.history.push_back(entry);
    }

    pub fn get(&self, index: usize) -> Option<&String> {
        self.history.get(index)
    }

    pub fn search(&self, prefix: &str) -> Option<String> {
        for entry in self.history.iter().rev() {
            if entry.starts_with(prefix) {
                return Some(entry.clone());
            }
        }
        None
    }

    pub fn search_all(&self, prefix: &str) -> Vec<String> {
        self.history
            .iter()
            .rev()
            .filter(|entry| entry.starts_with(prefix))
            .cloned()
            .collect()
    }

    pub fn len(&self) -> usize {
        self.history.len()
    }

    pub fn is_empty(&self) -> bool {
        self.history.is_empty()
    }

    pub fn clear(&mut self) {
        self.history.clear();
    }

    /// Writes the history to `path`, one entry per line — the format
    /// [`ReplHistory::load_from_file`] reads back.
    ///
    /// The path is anything that can name a file, not only a `&str`. Naming it a
    /// `&str` meant a path that is not UTF-8 could not be saved at all, and every
    /// caller holding a `PathBuf` had to convert lossily first to use it.
    pub fn save_to_file(&self, path: impl AsRef<Path>) -> std::io::Result<()> {
        use std::fs::File;
        use std::io::Write;

        let mut file = File::create(path)?;
        for entry in &self.history {
            writeln!(file, "{}", entry)?;
        }
        Ok(())
    }

    /// Reads a history file back, one entry per line.
    ///
    /// A read failure is the answer, not a short history: dropping the error
    /// with `map_while(Result::ok)` reported a corrupt file as a successful load
    /// of whatever happened to come before the bad line, so the caller could not
    /// tell a truncated history from a complete one.
    pub fn load_from_file(&mut self, path: impl AsRef<Path>) -> std::io::Result<()> {
        use std::fs::File;
        use std::io::{BufRead, BufReader};

        let file = File::open(path)?;
        let reader = BufReader::new(file);

        for line in reader.lines() {
            let line = line?;
            if !line.trim().is_empty() {
                self.push(line);
            }
        }

        Ok(())
    }
}

impl Default for ReplHistory {
    fn default() -> Self {
        Self::new(1000)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::ReplHistory;

    /// A scratch name no other test in this binary has taken.
    fn scratch_name(prefix: &str) -> String {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let serial = COUNTER.fetch_add(1, Ordering::SeqCst);
        format!("{}-{}", prefix, serial)
    }

    /// Where the history files under test are written, inside the project
    /// checkout rather than in a user's temp directory.
    fn scratch_dir() -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/tmp/repl-history");
        fs::create_dir_all(&dir).expect("scratch directory should be creatable");
        dir
    }

    /// Where the history file under test is written.
    fn scratch_path(name: &str) -> PathBuf {
        scratch_dir().join(format!("{}.txt", scratch_name(name)))
    }

    fn filled(entries: &[&str]) -> ReplHistory {
        let mut history = ReplHistory::new(100);
        for entry in entries {
            history.push((*entry).to_string());
        }
        history
    }

    #[test]
    fn push_keeps_entries_in_order_and_reachable_by_index() {
        let history = filled(&["first", "second", "third"]);

        assert_eq!(history.len(), 3);
        assert_eq!(history.get(0).map(String::as_str), Some("first"));
        assert_eq!(history.get(2).map(String::as_str), Some("third"));
        assert!(!history.is_empty());
    }

    /// A line typed twice in a row is one line of history, not two.
    #[test]
    fn push_suppresses_a_consecutive_duplicate() {
        let history = filled(&["set x to 1", "set x to 1", "set x to 1"]);

        assert_eq!(history.len(), 1);
        assert_eq!(history.get(0).map(String::as_str), Some("set x to 1"));
    }

    /// Only *consecutive* duplicates collapse: the same line run again later is
    /// a separate thing the user did.
    #[test]
    fn push_keeps_a_repeated_line_that_is_not_consecutive() {
        let history = filled(&["set x to 1", "set y to 2", "set x to 1"]);

        assert_eq!(history.len(), 3);
        assert_eq!(history.get(2).map(String::as_str), Some("set x to 1"));
    }

    #[test]
    fn push_evicts_the_oldest_entry_at_max_size() {
        let mut history = ReplHistory::new(2);
        history.push("one".to_string());
        history.push("two".to_string());
        history.push("three".to_string());

        assert_eq!(history.len(), 2, "the limit is a limit");
        assert_eq!(
            history.get(0).map(String::as_str),
            Some("two"),
            "the oldest entry is the one dropped"
        );
        assert_eq!(history.get(1).map(String::as_str), Some("three"));
    }

    /// Two entries that differ only in case are not duplicates, and the older
    /// one is still evicted before the newer one is stored.
    #[test]
    fn push_evicts_before_it_adds_so_a_full_history_never_exceeds_its_limit() {
        let mut history = ReplHistory::new(1);
        history.push("say \"Hello\"".to_string());
        history.push("say \"hello\"".to_string());

        assert_eq!(history.len(), 1);
        assert_eq!(history.get(0).map(String::as_str), Some("say \"hello\""));
    }

    #[test]
    fn search_returns_the_most_recent_match() {
        let history = filled(&["set x to 1", "say \"hi\"", "set y to 2"]);

        assert_eq!(
            history.search("set"),
            Some("set y to 2".to_string()),
            "the newest matching line is the one recalled"
        );
        assert_eq!(history.search("say"), Some("say \"hi\"".to_string()));
    }

    #[test]
    fn search_of_a_prefix_that_matches_nothing_returns_none() {
        let history = filled(&["set x to 1"]);

        assert_eq!(history.search("zz"), None);
        assert!(
            history.search_all("zz").is_empty(),
            "and no search_all hits either"
        );
    }

    #[test]
    fn search_all_lists_every_match_newest_first() {
        let history = filled(&["set x to 1", "say \"hi\"", "set y to 2"]);

        assert_eq!(
            history.search_all("set"),
            vec!["set y to 2".to_string(), "set x to 1".to_string()]
        );
    }

    #[test]
    fn save_to_file_and_load_from_file_round_trip() {
        let path = scratch_path("round-trip");
        let _ = fs::remove_file(&path);

        let saved = filled(&["set x to 1", "say \"hi\""]);
        saved
            .save_to_file(&path)
            .expect("history should be writable to the scratch path");

        let mut loaded = ReplHistory::new(100);
        loaded
            .load_from_file(&path)
            .expect("history should be readable back");

        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded.get(0).map(String::as_str), Some("set x to 1"));
        assert_eq!(loaded.get(1).map(String::as_str), Some("say \"hi\""));
        assert_eq!(loaded.search("set"), saved.search("set"));

        let _ = fs::remove_file(&path);
    }

    /// The file format is one entry per line, so a history that carries non-ASCII
    /// text has to come back byte-for-byte rather than mangled or re-encoded.
    #[test]
    fn edge_unicode_entries_survive_the_history_file() {
        let path = scratch_path("unicode");
        let _ = fs::remove_file(&path);

        let entry = "say \"héllo — 世界 🌍\"";
        let saved = filled(&[entry, "set café to 1"]);
        saved
            .save_to_file(&path)
            .expect("unicode history should be writable");

        let mut loaded = ReplHistory::new(100);
        loaded
            .load_from_file(&path)
            .expect("unicode history should be readable back");

        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded.get(0).map(String::as_str), Some(entry));
        assert_eq!(loaded.get(1).map(String::as_str), Some("set café to 1"));

        let _ = fs::remove_file(&path);
    }

    /// A file with blank lines in it — which a hand-edited history always has —
    /// must not become a history of empty entries.
    #[test]
    fn edge_blank_lines_in_the_history_file_are_skipped() {
        let path = scratch_path("blank-lines");
        fs::write(&path, "set x to 1\n\n   \nsay \"hi\"\n").expect("the file should be writable");

        let mut loaded = ReplHistory::new(100);
        loaded
            .load_from_file(&path)
            .expect("the history file should load");

        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded.get(1).map(String::as_str), Some("say \"hi\""));

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn edge_empty_history_answers_every_question_without_panicking() {
        let history = ReplHistory::new(10);

        assert!(history.is_empty());
        assert_eq!(history.len(), 0);
        assert_eq!(history.get(0), None, "an empty history has no first entry");
        assert_eq!(history.search("anything"), None);
        assert!(history.search_all("").is_empty());
    }

    /// Reading a history file that is not there is an error, not a panic and not
    /// an empty history silently accepted as a good load.
    #[test]
    fn edge_load_of_a_missing_file_is_reported_as_an_error() {
        let path = scratch_path("never-written");
        let _ = fs::remove_file(&path);
        assert!(
            !path.exists(),
            "the path must be absent for this to mean anything"
        );

        let mut loaded = ReplHistory::new(10);
        assert!(
            loaded.load_from_file(&path).is_err(),
            "reading a history file that does not exist must fail"
        );
        assert!(loaded.is_empty(), "and must not invent entries");
    }

    /// A file that stops being readable partway through is an error, not a short
    /// history reported as a good load. The unreadable line here is invalid UTF-8,
    /// which is what a hand-edited or truncated history file actually contains.
    #[test]
    fn edge_a_history_file_that_stops_being_readable_is_not_a_successful_load() {
        let path = scratch_path("corrupt");
        let mut bytes = b"set x to 1\n".to_vec();
        bytes.extend_from_slice(&[0xff, 0xfe, b'\n']);
        bytes.extend_from_slice(b"say \"hi\"\n");
        fs::write(&path, bytes).expect("the file should be writable");

        let mut loaded = ReplHistory::new(100);
        let outcome = loaded.load_from_file(&path);

        assert!(
            outcome.is_err(),
            "a file with an unreadable line must not load as if it had been read whole"
        );

        let _ = fs::remove_file(&path);
    }

    /// Everything before the unreadable line is still there: the error says the
    /// load did not finish, and the entries already read are not thrown away.
    #[test]
    fn edge_the_entries_read_before_a_bad_line_are_kept() {
        let path = scratch_path("corrupt-partial");
        let mut bytes = b"set x to 1\nsay \"hi\"\n".to_vec();
        bytes.extend_from_slice(&[0xff, b'\n']);
        fs::write(&path, bytes).expect("the file should be writable");

        let mut loaded = ReplHistory::new(100);
        assert!(loaded.load_from_file(&path).is_err());
        assert_eq!(loaded.len(), 2, "the readable lines were still read");
        assert_eq!(loaded.get(0).map(String::as_str), Some("set x to 1"));

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn clear_empties_the_history() {
        let mut history = filled(&["one", "two"]);

        history.clear();

        assert!(history.is_empty());
        assert_eq!(history.search("one"), None);
    }

    /// A limit of zero means "keep nothing", so a push into it must store
    /// nothing rather than the single entry that pop-then-push leaves behind.
    #[test]
    fn edge_zero_size_history_stores_nothing() {
        let mut history = ReplHistory::new(0);

        history.push("set x to 1".to_string());

        assert_eq!(
            history.len(),
            0,
            "a zero-sized history cannot hold an entry"
        );
        assert!(history.is_empty());
    }

    /// A path is a path, not text. Naming the parameter `&str` meant a history
    /// file whose own name is not UTF-8 could not be saved or loaded at all, and
    /// every caller holding a `PathBuf` had to convert lossily to name one.
    #[cfg(unix)]
    #[test]
    fn edge_a_file_whose_own_name_is_not_utf8_can_be_saved_and_loaded() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let mut name = scratch_name("non-utf8").into_bytes();
        name.extend_from_slice(&[0xff, 0xfe]);
        let path = scratch_dir().join(OsStr::from_bytes(&name));
        let _ = fs::remove_file(&path);

        let saved = filled(&["set x to 1"]);
        saved
            .save_to_file(&path)
            .expect("a non-UTF-8 path is still a path");

        let mut loaded = ReplHistory::new(100);
        loaded
            .load_from_file(&path)
            .expect("and it reads back the same way");

        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded.get(0).map(String::as_str), Some("set x to 1"));

        let _ = fs::remove_file(&path);
    }
}
