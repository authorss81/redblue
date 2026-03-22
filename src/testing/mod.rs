pub mod runner;
pub mod harness;
pub mod assertions;
pub mod reporter;

pub use runner::TestRunner;
pub use harness::TestHarness;
pub use assertions::*;
pub use reporter::TestReporter;

use crate::vm::Vm;
use crate::error::Result;
use crate::value::Value;

pub fn run_test_file(path: &str) -> Result<TestResults> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| crate::error::Error::Io(format!("Failed to read test file: {}", e)))?;
    
    let mut harness = TestHarness::new();
    harness.run_source(&source)?;
    
    Ok(harness.results())
}

pub fn run_all_tests() -> Result<TestResults> {
    let mut harness = TestHarness::new();
    
    let test_files = find_test_files("tests/")?;
    
    for file in test_files {
        if file.ends_with("_test.rs") || file.ends_with(".rb") {
            if let Err(e) = harness.run_file(&file) {
                harness.add_error(e.to_string());
            }
        }
    }
    
    Ok(harness.results())
}

fn find_test_files(dir: &str) -> Result<Vec<String>> {
    let mut files = Vec::new();
    
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_dir() {
                files.extend(find_test_files(&path.to_string_lossy())?);
            } else if let Some(ext) = path.extension() {
                if ext == "rs" || ext == "rb" {
                    files.push(path.to_string_lossy().to_string());
                }
            }
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
        if self.total == 0 { return 0.0; }
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
