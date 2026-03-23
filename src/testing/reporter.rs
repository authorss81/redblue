use crate::testing::TestResults;

pub trait Reporter {
    fn report(&self, results: &TestResults);
}

pub struct PrettyReporter {
    show_passed: bool,
    show_failed: bool,
    _show_skipped: bool,
    color: bool,
}

impl PrettyReporter {
    pub fn new() -> Self {
        Self {
            show_passed: false,
            show_failed: true,
            _show_skipped: true,
            color: true,
        }
    }

    pub fn show_passed(mut self, show: bool) -> Self {
        self.show_passed = show;
        self
    }

    pub fn show_failed(mut self, show: bool) -> Self {
        self.show_failed = show;
        self
    }

    pub fn color(mut self, color: bool) -> Self {
        self.color = color;
        self
    }

    fn colorize(&self, text: &str, color: &str) -> String {
        if self.color {
            match color {
                "red" => format!("\x1B[31m{}\x1B[0m", text),
                "green" => format!("\x1B[32m{}\x1B[0m", text),
                "yellow" => format!("\x1B[33m{}\x1B[0m", text),
                "blue" => format!("\x1B[34m{}\x1B[0m", text),
                "bold" => format!("\x1B[1m{}\x1B[0m", text),
                _ => text.to_string(),
            }
        } else {
            text.to_string()
        }
    }
}

impl Default for PrettyReporter {
    fn default() -> Self {
        Self::new()
    }
}

impl Reporter for PrettyReporter {
    fn report(&self, results: &TestResults) {
        println!();

        let summary = format!(
            "Test Results: {} total, {} passed, {} failed, {} skipped ({:.1}% success)",
            results.total,
            results.passed,
            results.failed,
            results.skipped,
            results.success_rate()
        );

        if results.failed > 0 {
            println!("{}", self.colorize(&summary, "red"));
        } else if results.passed > 0 {
            println!("{}", self.colorize(&summary, "green"));
        } else {
            println!("{}", summary);
        }

        println!("Duration: {}ms", results.duration_ms);

        if !results.errors.is_empty() {
            println!();
            println!("{}", self.colorize("Failures:", "bold"));
            println!("{}", "=".repeat(60));

            for (i, error) in results.errors.iter().enumerate() {
                println!();
                println!(
                    "{}: {}",
                    self.colorize(&format!("{}. {}", i + 1, error.test_name), "red"),
                    self.colorize("FAILED", "red")
                );

                if !error.file.is_empty() {
                    if let Some(line) = error.line {
                        println!("  Location: {}:{}", error.file, line);
                    } else {
                        println!("  Location: {}", error.file);
                    }
                }

                println!("  Error: {}", error.message);

                if let Some(expected) = &error.expected {
                    println!("  Expected: {}", expected);
                }

                if let Some(actual) = &error.actual {
                    println!("  Actual: {}", actual);
                }
            }
        }

        println!();

        if results.failed > 0 {
            std::process::exit(1);
        }
    }
}

pub struct JsonReporter;

impl JsonReporter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for JsonReporter {
    fn default() -> Self {
        Self::new()
    }
}

impl Reporter for JsonReporter {
    fn report(&self, results: &TestResults) {
        println!("{{");
        println!("  \"total\": {},", results.total);
        println!("  \"passed\": {},", results.passed);
        println!("  \"failed\": {},", results.failed);
        println!("  \"skipped\": {},", results.skipped);
        println!("  \"success_rate\": {:.2},", results.success_rate());
        println!("  \"duration_ms\": {},", results.duration_ms);

        if !results.errors.is_empty() {
            println!("  \"errors\": [");
            for (i, error) in results.errors.iter().enumerate() {
                println!("    {{");
                println!("      \"test\": \"{}\",", error.test_name);
                println!("      \"file\": \"{}\",", error.file);
                if let Some(line) = error.line {
                    println!("      \"line\": {},", line);
                }
                println!(
                    "      \"message\": \"{}\",",
                    error.message.replace('"', "\\\"")
                );
                if let Some(expected) = &error.expected {
                    println!("      \"expected\": \"{}\",", expected.replace('"', "\\\""));
                }
                if let Some(actual) = &error.actual {
                    println!("      \"actual\": \"{}\"", actual.replace('"', "\\\""));
                }
                println!(
                    "    }}{}",
                    if i < results.errors.len() - 1 {
                        ","
                    } else {
                        ""
                    }
                );
            }
            println!("  ],");
        } else {
            println!("  \"errors\": [],");
        }

        println!("  \"timestamp\": \"{}\"", chrono::Utc::now().to_rfc3339());
        println!("}}");
    }
}

pub struct TapReporter;

impl TapReporter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TapReporter {
    fn default() -> Self {
        Self::new()
    }
}

impl Reporter for TapReporter {
    fn report(&self, results: &TestResults) {
        println!("TAP version 13");
        println!("1..{}", results.total);

        let mut count = 0;
        for _ in 0..results.passed {
            count += 1;
            println!("ok {} - passed", count);
        }

        for error in &results.errors {
            count += 1;
            println!("not ok {} - {}", count, error.test_name);
            println!("  ---");
            println!("  message: {}", error.message);
            if let Some(expected) = &error.expected {
                println!("  expected: {}", expected);
            }
            if let Some(actual) = &error.actual {
                println!("  actual: {}", actual);
            }
            println!("  ---");
        }

        for _ in 0..results.skipped {
            count += 1;
            println!("ok {} - # SKIP", count);
        }
    }
}

pub struct CompactReporter;

impl CompactReporter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CompactReporter {
    fn default() -> Self {
        Self::new()
    }
}

impl Reporter for CompactReporter {
    fn report(&self, results: &TestResults) {
        if results.failed > 0 {
            println!("FAIL: {}/{} tests failed", results.failed, results.total);
        } else {
            println!("PASS: All {} tests passed", results.total);
        }
    }
}
