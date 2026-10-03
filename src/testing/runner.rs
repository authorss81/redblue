use crate::testing::{TestError, TestResults};
use crate::value::Value;
use std::time::Instant;

pub struct TestRunner {
    results: TestResults,
    current_test: Option<String>,
    current_file: String,
    start_time: Instant,
}

impl TestRunner {
    pub fn new() -> Self {
        Self {
            results: TestResults::new(),
            current_test: None,
            current_file: String::new(),
            start_time: Instant::now(),
        }
    }

    pub fn run_test<F>(&mut self, name: &str, test_fn: F)
    where
        F: FnOnce() -> Result<(), TestAssertionError>,
    {
        self.current_test = Some(name.to_string());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(test_fn));

        match result {
            Ok(Ok(())) => {
                self.results.add_pass();
            }
            Ok(Err(e)) => {
                self.results.add_fail(TestError {
                    test_name: name.to_string(),
                    file: self.current_file.clone(),
                    line: None,
                    message: e.message,
                    expected: e.expected,
                    actual: e.actual,
                });
            }
            Err(_) => {
                self.results.add_fail(TestError {
                    test_name: name.to_string(),
                    file: self.current_file.clone(),
                    line: None,
                    message: "Test panicked".to_string(),
                    expected: None,
                    actual: None,
                });
            }
        }

        self.current_test = None;
    }

    pub fn run_benchmark<F>(&mut self, name: &str, iterations: usize, mut fn_: F)
    where
        F: FnMut(),
    {
        let start = Instant::now();

        for _ in 0..iterations {
            fn_();
        }

        let elapsed = start.elapsed();
        let avg_ns = elapsed.as_nanos() / iterations as u128;

        println!(
            "Benchmark: {} - {} ns/iter ({} iterations)",
            name, avg_ns, iterations
        );
    }

    pub fn skip(&mut self, name: &str, reason: &str) {
        println!("SKIP: {} - {}", name, reason);
        self.results.add_skip();
    }

    pub fn set_file(&mut self, file: &str) {
        self.current_file = file.to_string();
    }

    pub fn results(&self) -> TestResults {
        let mut results = self.results.clone();
        results.duration_ms = self.start_time.elapsed().as_millis();
        results
    }
}

impl Default for TestRunner {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct TestAssertionError {
    pub message: String,
    pub expected: Option<String>,
    pub actual: Option<String>,
}

impl TestAssertionError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            expected: None,
            actual: None,
        }
    }

    pub fn expected<T: std::fmt::Display>(expected: T) -> Self {
        Self {
            message: format!("Expected {}", expected),
            expected: Some(expected.to_string()),
            actual: None,
        }
    }

    pub fn but_was<T: std::fmt::Display>(actual: T) -> Self {
        Self {
            message: format!("Expected but was: {}", actual),
            expected: None,
            actual: Some(actual.to_string()),
        }
    }
}

pub fn assert_true(value: bool) -> Result<(), TestAssertionError> {
    if value {
        Ok(())
    } else {
        Err(TestAssertionError::new("Expected true but was false"))
    }
}

pub fn assert_false(value: bool) -> Result<(), TestAssertionError> {
    if !value {
        Ok(())
    } else {
        Err(TestAssertionError::new("Expected false but was true"))
    }
}

pub fn assert_eq<T: PartialEq + std::fmt::Display + std::fmt::Debug>(
    expected: T,
    actual: T,
) -> Result<(), TestAssertionError> {
    if expected == actual {
        Ok(())
    } else {
        Err(TestAssertionError {
            message: format!("Expected {:?} but got {:?}", expected, actual),
            expected: Some(expected.to_string()),
            actual: Some(actual.to_string()),
        })
    }
}

pub fn assert_ne<T: PartialEq + std::fmt::Display + std::fmt::Debug>(
    not_expected: T,
    actual: T,
) -> Result<(), TestAssertionError> {
    if not_expected != actual {
        Ok(())
    } else {
        Err(TestAssertionError {
            message: format!("Expected NOT {:?} but got it", not_expected),
            expected: None,
            actual: Some(actual.to_string()),
        })
    }
}

pub fn assert_none<T: std::fmt::Debug>(value: &Option<T>) -> Result<(), TestAssertionError> {
    match value {
        None => Ok(()),
        Some(v) => Err(TestAssertionError {
            message: format!("Expected None but got Some({:?})", v),
            expected: Some("None".to_string()),
            actual: Some(format!("Some({:?})", v)),
        }),
    }
}

pub fn assert_some<T: std::fmt::Debug>(value: &Option<T>) -> Result<(), TestAssertionError> {
    match value {
        Some(_) => Ok(()),
        None => Err(TestAssertionError::new("Expected Some but got None")),
    }
}

pub fn assert_panics<F>(f: F) -> Result<(), TestAssertionError>
where
    F: FnOnce(),
{
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));

    match result {
        Err(_) => Ok(()),
        Ok(_) => Err(TestAssertionError::new("Expected panic but none occurred")),
    }
}

pub fn assert_not_panics<F>(f: F) -> Result<(), TestAssertionError>
where
    F: FnOnce(),
{
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));

    match result {
        Ok(_) => Ok(()),
        Err(e) => Err(TestAssertionError {
            message: format!("Expected no panic but got: {:?}", e),
            expected: Some("no panic".to_string()),
            actual: Some(format!("panic: {:?}", e)),
        }),
    }
}

pub fn assert_approx_eq(
    expected: f64,
    actual: f64,
    epsilon: f64,
) -> Result<(), TestAssertionError> {
    if (expected - actual).abs() < epsilon {
        Ok(())
    } else {
        Err(TestAssertionError {
            message: format!("Expected ~{} but got {}", expected, actual),
            expected: Some(expected.to_string()),
            actual: Some(actual.to_string()),
        })
    }
}

pub fn assert_contains(haystack: &str, needle: &str) -> Result<(), TestAssertionError> {
    if haystack.contains(needle) {
        Ok(())
    } else {
        Err(TestAssertionError {
            message: format!("Expected '{}' to contain '{}'", haystack, needle),
            expected: Some(needle.to_string()),
            actual: Some(haystack.to_string()),
        })
    }
}

pub fn assert_matches_regex(pattern: &str, text: &str) -> Result<(), TestAssertionError> {
    use regex::Regex;

    match Regex::new(pattern) {
        Ok(re) => {
            if re.is_match(text) {
                Ok(())
            } else {
                Err(TestAssertionError {
                    message: format!("Expected '{}' to match pattern '{}'", text, pattern),
                    expected: Some(pattern.to_string()),
                    actual: Some(text.to_string()),
                })
            }
        }
        Err(e) => Err(TestAssertionError::new(format!("Invalid regex: {}", e))),
    }
}

pub fn assert_len<T: IntoIterator>(iter: T, expected_len: usize) -> Result<(), TestAssertionError> {
    let len = iter.into_iter().count();

    if len == expected_len {
        Ok(())
    } else {
        Err(TestAssertionError {
            message: format!("Expected length {} but got {}", expected_len, len),
            expected: Some(expected_len.to_string()),
            actual: Some(len.to_string()),
        })
    }
}

pub fn assert_type<T: 'static>(value: &Value) -> Result<(), TestAssertionError> {
    use std::any::TypeId;

    let value_type = match value {
        Value::Nothing => TypeId::of::<()>(),
        Value::Number(_) => TypeId::of::<f64>(),
        Value::Text(_) => TypeId::of::<String>(),
        Value::YesNo(_) => TypeId::of::<bool>(),
        Value::List(_) => TypeId::of::<Vec<Value>>(),
        Value::Record(_) => TypeId::of::<std::collections::HashMap<String, Value>>(),
        Value::Object(_, _) => TypeId::of::<Value>(),
        Value::Function(_, _) => TypeId::of::<Value>(),
        Value::Builtin(_) => TypeId::of::<Value>(),
    };

    let expected_type = TypeId::of::<T>();

    if value_type == expected_type {
        Ok(())
    } else {
        Err(TestAssertionError {
            message: format!(
                "Expected type {} but got {:?}",
                std::any::type_name::<T>(),
                value
            ),
            expected: Some(std::any::type_name::<T>().to_string()),
            actual: Some(format!("{:?}", value)),
        })
    }
}
