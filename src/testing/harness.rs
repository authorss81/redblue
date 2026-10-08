use crate::analyzer;
use crate::error::{Error, Result};
use crate::lexer::Lexer;
use crate::parser::{Parser, Program, Stmt};
use crate::testing::{TestError, TestFailure, TestResults};
use std::collections::HashMap;

pub struct TestHarness {
    results: TestResults,
    _globals: HashMap<String, crate::value::Value>,
    _test_context: HashMap<String, crate::value::Value>,
}

impl TestHarness {
    pub fn new() -> Self {
        Self {
            results: TestResults::new(),
            _globals: crate::stdlib::builtins(),
            _test_context: HashMap::new(),
        }
    }

    pub fn run_source(&mut self, source: &str) -> Result<()> {
        // Real `test "..." ... end` blocks first: they are the language's own
        // test statement, so the parser tells us where each block ends rather
        // than a line scanner guessing.
        self.run_test_blocks(source);

        let lines: Vec<&str> = source.lines().collect();
        let mut i = 0;

        while i < lines.len() {
            let line = lines[i].trim();

            if line.starts_with("// test ") || line.starts_with("# test ") {
                let test_name = line
                    .trim_start_matches("// test ")
                    .trim_start_matches("# test ");
                self.run_single_test(&lines, &mut i, test_name)?;
            } else if line.starts_with("// bench ") || line.starts_with("# bench ") {
                let bench_name = line
                    .trim_start_matches("// bench ")
                    .trim_start_matches("# bench ");
                self.run_benchmark(&lines, &mut i, bench_name)?;
            } else if line.starts_with("// skip") || line.starts_with("# skip") {
                let reason = if i + 1 < lines.len() {
                    lines[i + 1].trim().to_string()
                } else {
                    "No reason provided".to_string()
                };
                self.results.add_skip();
                println!("SKIP: {} - {}", lines.get(i).unwrap_or(&""), reason);
                i += 1;
            } else {
                i += 1;
            }
        }

        Ok(())
    }

    pub fn run_file(&mut self, path: &str) -> Result<()> {
        let source = std::fs::read_to_string(path)
            .map_err(|e| Error::Io(format!("Failed to read {}: {}", path, e)))?;

        self.run_source(&source)
    }

    /// Runs every `test "..." ... end` block the parser found in `source`.
    ///
    /// Each block is analysed and executed on its own so that one failing
    /// `expect` fails one test instead of aborting the whole file. Block
    /// boundaries come from the parser, so a nested `if`/`for`/`try` inside a
    /// test cannot terminate the block early.
    fn run_test_blocks(&mut self, source: &str) {
        let program = match Lexer::tokenize(source).and_then(|tokens| Parser::new(tokens).parse()) {
            Ok(program) => program,
            Err(e) => {
                // A marker-style file (`// test "..."`) declares no real block,
                // so its commented bodies are not required to parse and the
                // marker scan below owns that file. When a real block *was*
                // asked for and cannot be read, that is a failure, not silence.
                if declares_test_block(source) {
                    self.add_error(e.to_string());
                }
                return;
            }
        };

        let blocks: Vec<(String, usize, Vec<Stmt>)> = program
            .statements
            .into_iter()
            .filter_map(|stmt| match stmt.statement {
                crate::parser::Statement::Test { name, body } => Some((name, stmt.span.line, body)),
                _ => None,
            })
            .collect();

        for (name, line, body) in blocks {
            self.report_test(&name, line, self.execute_test_statements(body));
        }
    }

    fn run_single_test(&mut self, lines: &[&str], i: &mut usize, test_name: &str) -> Result<()> {
        let mut test_lines = Vec::new();
        *i += 1;

        while *i < lines.len() {
            let line = lines[*i];
            if line.trim().starts_with("// end") || line.trim() == "end" {
                break;
            }
            test_lines.push(line);
            *i += 1;
        }

        let test_code = test_lines.join("\n");

        self.report_test(test_name, *i, self.execute_test_code(&test_code));

        Ok(())
    }

    /// Records one test outcome: a pass, an assertion failure, or a fault.
    fn report_test(
        &mut self,
        name: &str,
        line: usize,
        outcome: std::result::Result<(), TestFailure>,
    ) {
        match outcome {
            Ok(()) => {
                self.results.add_pass();
                print!(".");
            }
            Err(TestFailure::Assertion(failure)) => {
                self.results.add_fail(TestError {
                    test_name: name.to_string(),
                    file: "inline".to_string(),
                    line: Some(line),
                    message: failure.message.clone(),
                    expected: failure.expected.clone(),
                    actual: failure.actual.clone(),
                });
                print!("F");
            }
            Err(TestFailure::Error(e)) => {
                self.results.add_fail(TestError {
                    test_name: name.to_string(),
                    file: "inline".to_string(),
                    line: Some(line),
                    message: e.to_string(),
                    expected: None,
                    actual: None,
                });
                print!("F");
            }
        }
    }

    fn run_benchmark(&mut self, lines: &[&str], i: &mut usize, bench_name: &str) -> Result<()> {
        let mut bench_lines = Vec::new();
        *i += 1;

        while *i < lines.len() {
            let line = lines[*i];
            if line.trim().starts_with("// end") || line.trim() == "end" {
                break;
            }
            bench_lines.push(line);
            *i += 1;
        }

        println!("BENCHMARK: {} - Running...", bench_name);

        let bench_code = bench_lines.join("\n");
        let iterations = 1000;

        let start = std::time::Instant::now();

        for _ in 0..iterations {
            let _ = self.execute_test_code(&bench_code);
        }

        let elapsed = start.elapsed();
        let avg_ns = elapsed.as_nanos() / iterations as u128;

        println!(
            "  {}: {} ns/iter ({} iterations)",
            bench_name, avg_ns, iterations
        );

        Ok(())
    }

    fn execute_test_code(&self, code: &str) -> std::result::Result<(), TestFailure> {
        let program = Lexer::tokenize(code)
            .and_then(|tokens| Parser::new(tokens).parse())
            .and_then(|program| {
                analyzer::analyze(&program)?;
                Ok(program)
            })
            .map_err(TestFailure::Error)?;

        self.execute_test_program(&program)
    }

    /// Same as [`Self::execute_test_code`], for a block the parser already
    /// turned into statements.
    fn execute_test_statements(&self, body: Vec<Stmt>) -> std::result::Result<(), TestFailure> {
        let program = Program { statements: body };

        analyzer::analyze(&program).map_err(TestFailure::Error)?;

        self.execute_test_program(&program)
    }

    fn execute_test_program(&self, program: &Program) -> std::result::Result<(), TestFailure> {
        let (mut vm, run) = crate::interpreter::run_isolated(program);

        if run.is_err() {
            // A failed `expect` carries more than the rendered message; keep the
            // expected and actual values so the reporter can name both.
            if let Some(failure) = vm.take_expectation_failure() {
                return Err(TestFailure::Assertion(failure));
            }
        }

        run.map(|_| ()).map_err(TestFailure::Error)
    }

    pub fn add_error(&mut self, error: String) {
        self.results.add_fail(TestError {
            test_name: "File execution".to_string(),
            file: "unknown".to_string(),
            line: None,
            message: error,
            expected: None,
            actual: None,
        });
    }

    pub fn results(&self) -> TestResults {
        self.results.clone()
    }
}

impl Default for TestHarness {
    fn default() -> Self {
        Self::new()
    }
}

/// True when `source` opens a real `test "..."` block on a line of its own.
///
/// Marker-style tests are written `// test "..."`, which never starts with
/// `test`, so this separates the two discovery conventions.
fn declares_test_block(source: &str) -> bool {
    source
        .lines()
        .map(str::trim_start)
        .any(|line| line.starts_with("test "))
}
