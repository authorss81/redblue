use crate::testing::{TestResults, TestError};
use crate::lexer::Lexer;
use crate::parser::Parser;
use crate::analyzer;
use crate::vm::Vm;
use crate::error::{Error, Result};
use std::collections::HashMap;

pub struct TestHarness {
    results: TestResults,
    globals: HashMap<String, crate::value::Value>,
    test_context: HashMap<String, crate::value::Value>,
}

impl TestHarness {
    pub fn new() -> Self {
        Self {
            results: TestResults::new(),
            globals: stdlib::all_modules(),
            test_context: HashMap::new(),
        }
    }
    
    pub fn run_source(&mut self, source: &str) -> Result<()> {
        let lines: Vec<&str> = source.lines().collect();
        let mut i = 0;
        
        while i < lines.len() {
            let line = lines[i].trim();
            
            if line.starts_with("// test ") || line.starts_with("# test ") {
                let test_name = line.trim_start_matches("// test ")
                    .trim_start_matches("# test ");
                self.run_single_test(lines, &mut i, test_name)?;
            } else if line.starts_with("// bench ") || line.starts_with("# bench ") {
                let bench_name = line.trim_start_matches("// bench ")
                    .trim_start_matches("# bench ");
                self.run_benchmark(lines, &mut i, bench_name)?;
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
    
    fn run_single_test(&mut self, lines: Vec<&str>, i: &mut usize, test_name: &str) -> Result<()> {
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
        
        match self.execute_test_code(&test_code) {
            Ok(()) => {
                self.results.add_pass();
                print!(".");
            },
            Err(e) => {
                self.results.add_fail(TestError {
                    test_name: test_name.to_string(),
                    file: "inline".to_string(),
                    line: Some(*i),
                    message: e.to_string(),
                    expected: None,
                    actual: None,
                });
                print!("F");
            }
        }
        
        Ok(())
    }
    
    fn run_benchmark(&mut self, lines: Vec<&str>, i: &mut usize, bench_name: &str) -> Result<()> {
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
        
        println!("  {}: {} ns/iter ({} iterations)", bench_name, avg_ns, iterations);
        
        Ok(())
    }
    
    fn execute_test_code(&self, code: &str) -> Result<()> {
        let tokens = Lexer::tokenize(code)?;
        let mut parser = Parser::new(tokens);
        let program = parser.parse()?;
        analyzer::analyze(&program)?;
        
        let mut vm = Vm::new();
        vm.run(&program)?;
        
        Ok(())
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
