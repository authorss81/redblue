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
pub fn run_tests_in(dir: impl AsRef<Path>) -> Result<TestResults> {
    let mut harness = TestHarness::new();
    let dir = dir.as_ref();

    let test_files = find_test_files(dir)?;
    if test_files.is_empty() {
        return Err(Error::Io(format!(
            "No Redblue test files found in '{}'",
            dir.display()
        )));
    }

    for file in test_files {
        if let Err(e) = harness.run_file(&file) {
            harness.add_error(e.to_string());
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
pub fn find_test_files(dir: impl AsRef<Path>) -> Result<Vec<PathBuf>> {
    let mut files = collect_test_files(dir.as_ref())?;
    files.sort();
    Ok(files)
}

fn collect_test_files(dir: &Path) -> Result<Vec<PathBuf>> {
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
        if path.is_dir() {
            files.extend(collect_test_files(&path)?);
        } else if path.extension() == Some("rb".as_ref()) {
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
