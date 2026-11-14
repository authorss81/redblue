pub mod assertions;
pub mod harness;
pub mod reporter;
pub mod runner;

pub use assertions::*;
pub use harness::TestHarness;
pub use reporter::PrettyReporter;
pub use runner::TestRunner;

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// The most directory levels [`find_test_files`] will read, counting the root
/// as level 0. A directory at level `MAX_TEST_DIR_DEPTH` is refused before it
/// is opened.
///
/// Skipping directory symlinks stops a cycle, but a real tree can still be deep
/// enough to overflow the stack. A fixed bound turns that resource failure into
/// a reported error instead of a crash.
const MAX_TEST_DIR_DEPTH: usize = 64;

/// Why a single test body did not pass. An `Assertion` failure is a Redblue
/// `expect` that did not hold; an `Error` is a lexer/parser/analyzer/runtime
/// fault, which is a different kind of test failure.
#[derive(Debug)]
pub enum TestFailure {
    Assertion(TestAssertionError),
    Error(crate::error::Error),
}

impl std::fmt::Display for TestFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TestFailure::Assertion(failure) => write!(f, "{}", failure.message),
            TestFailure::Error(e) => write!(f, "{}", e),
        }
    }
}

pub fn run_test_file(path: &str) -> Result<TestResults> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| crate::error::Error::Io(format!("Failed to read test file: {}", e)))?;

    let mut harness = TestHarness::new();
    harness.run_source(&source)?;

    Ok(harness.results())
}

pub fn run_all_tests() -> Result<TestResults> {
    run_tests_in("tests/")
}

/// Runs every Redblue test file under `dir`, recursively.
///
/// [`run_all_tests`] is this against the checkout's own `tests/`; a project's
/// bare `rb test` is this against the `tests/` beside its manifest.
///
/// A directory with no test files is not an error here: it yields a zero-count
/// result, which is what [`run_all_tests`] has always returned for an empty
/// `tests/`. The command layer decides whether "no tests" is a failure.
pub fn run_tests_in(dir: impl AsRef<Path>) -> Result<TestResults> {
    let test_files = find_test_files(dir)?;
    run_test_files(&test_files)
}

/// Runs the already-discovered test files, preserving their order.
///
/// [`run_tests_in`] collects once and calls this, and callers that need to
/// inspect the listing first (to distinguish "no files" from "no tests") can
/// pass the same list here instead of walking the directory a second time.
pub fn run_test_files(files: &[PathBuf]) -> Result<TestResults> {
    let mut harness = TestHarness::new();

    for file in files {
        if let Err(e) = harness.run_file(file) {
            harness.add_source_error_at(file, &e);
        }
    }

    Ok(harness.results())
}

/// Collects every Redblue test file under `dir`, recursively.
///
/// Only `.rb` is collectable. The Redblue harness parses its input as
/// Redblue, so handing it Rust source — the `tests/*_test.rs` files that
/// belong to `cargo test` — can only ever produce noise.
///
/// A directory that does not exist collects nothing, because asking whether a
/// path exists is not the same as failing to read one. A directory that exists
/// but cannot be read is an error: silently skipping it would let a test run
/// report success after examining none of the files it was pointed at.
///
/// The result is sorted. `read_dir` yields entries in filesystem hash order,
/// which differs between machines and filesystems; an unsorted list would make
/// the reported test order — and therefore the failure output — unstable.
///
/// Symlinks are never followed, whether they name a directory or a file.
/// `Path::is_dir` dereferences a link, so a link such as `tests/loop -> .`
/// would make this walk recurse until the process overflowed its stack.
/// `DirEntry::file_type` reports the link itself, so a symlinked directory is
/// skipped and the walk can only descend into real directories. For the same
/// reason a symlinked `.rb` file is not a regular file and is not collected:
/// `tests/escape.rb -> /somewhere/outside.rb` would otherwise run a suite from
/// outside the project. The caller-supplied root is refused when it is itself
/// a link: `read_dir` would follow it and run tests from outside the project.
pub fn find_test_files(dir: impl AsRef<Path>) -> Result<Vec<PathBuf>> {
    let dir = without_trailing_separator(dir.as_ref());
    let mut files = collect_test_files(&dir, 0)?;
    files.sort();
    Ok(files)
}

/// Drops trailing separators (and a final `.`) so the root is inspected as
/// itself. `symlink_metadata("tests/")` resolves through the final link because
/// a trailing separator requires a directory, so it reports the target and the
/// symlink check never fires. `Path::components` normalises repeated and
/// trailing separators, including the one `run_all_tests` supplies.
fn without_trailing_separator(dir: &Path) -> PathBuf {
    dir.components().collect()
}

fn collect_test_files(dir: &Path, depth: usize) -> Result<Vec<PathBuf>> {
    if depth >= MAX_TEST_DIR_DEPTH {
        return Err(Error::Io(format!(
            "Test directory '{}' is nested {} levels deep, at the limit of {}",
            dir.display(),
            depth,
            MAX_TEST_DIR_DEPTH
        )));
    }

    // `read_dir` follows a symlink for the path it is handed, even though this
    // walk checks every entry below it. Inspect the root with `symlink_metadata`
    // so a symlinked `tests/` cannot run a suite from outside the project.
    match std::fs::symlink_metadata(dir) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(Error::Io(format!(
                "Refusing to search '{}': it is a symbolic link",
                dir.display()
            )))
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(Error::Io(format!(
                "Cannot inspect test directory '{}': {}",
                dir.display(),
                error
            )))
        }
    }

    let mut files = Vec::new();

    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(files),
        Err(error) => {
            return Err(Error::Io(format!(
                "Cannot read test directory '{}': {}",
                dir.display(),
                error
            )))
        }
    };

    for entry in entries {
        let entry = entry.map_err(|error| {
            Error::Io(format!(
                "Cannot read test directory '{}': {}",
                dir.display(),
                error
            ))
        })?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|error| {
            Error::Io(format!(
                "Cannot read test directory '{}': {}",
                dir.display(),
                error
            ))
        })?;
        if file_type.is_dir() {
            files.extend(collect_test_files(&path, depth + 1)?);
        } else if file_type.is_file() && path.extension() == Some("rb".as_ref()) {
            files.push(path);
        }
    }

    Ok(files)
}

#[derive(Debug, Clone)]
pub struct TestResults {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub errors: Vec<TestError>,
    pub duration_ms: u128,
}

impl TestResults {
    pub fn new() -> Self {
        Self {
            total: 0,
            passed: 0,
            failed: 0,
            skipped: 0,
            errors: Vec::new(),
            duration_ms: 0,
        }
    }

    pub fn add_pass(&mut self) {
        self.passed += 1;
        self.total += 1;
    }

    pub fn add_fail(&mut self, error: TestError) {
        self.failed += 1;
        self.total += 1;
        self.errors.push(error);
    }

    pub fn add_skip(&mut self) {
        self.skipped += 1;
        self.total += 1;
    }

    pub fn success_rate(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        (self.passed as f64 / self.total as f64) * 100.0
    }
}

impl Default for TestResults {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct TestError {
    pub test_name: String,
    pub file: String,
    pub line: Option<usize>,
    pub message: String,
    pub expected: Option<String>,
    pub actual: Option<String>,
}
